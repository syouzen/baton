//! Baton terminal core primitives.
//!
//! This crate intentionally starts with the data-plane rules from the spec:
//! bounded byte buffers, frame-paced coalescing, and fixed-memory scrollback.
//! UI/chrome and native GPU composition can evolve without changing these hot-path contracts.

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::vte::ansi::Processor as VteProcessor;
use portable_pty::{native_pty_system, Child, CommandBuilder, ExitStatus, MasterPty, PtySize};

/// Producer-side signal used to pause PTY reads before bytes are dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watermark {
    /// The ring reached capacity after accepting this many bytes from the attempted write.
    Full { accepted: usize },
}

/// A bounded FIFO byte ring for PTY output.
///
/// The ring never drops accepted bytes. If a write would exceed capacity it accepts only the
/// remaining room and returns [`Watermark::Full`], letting the caller pause PTY reads and rely on
/// OS pipe backpressure.
#[derive(Debug, Clone)]
pub struct ByteRing {
    buf: VecDeque<u8>,
    capacity: usize,
}

impl ByteRing {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "ByteRing capacity must be non-zero");
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn remaining_capacity(&self) -> usize {
        self.capacity - self.buf.len()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<usize, Watermark> {
        let room = self.remaining_capacity();
        let accepted = room.min(bytes.len());
        self.buf.extend(&bytes[..accepted]);

        if accepted < bytes.len() {
            Err(Watermark::Full { accepted })
        } else {
            Ok(accepted)
        }
    }

    pub fn drain_all(&mut self) -> Vec<u8> {
        self.buf.drain(..).collect()
    }
}

/// Frame-cadenced byte coalescer for the PTY data plane.
#[derive(Debug, Clone)]
pub struct Coalescer {
    ring: ByteRing,
    frame_interval: Duration,
    first_pending_at: Option<Duration>,
}

impl Coalescer {
    pub fn new(capacity: usize, frame_interval: Duration) -> Self {
        assert!(!frame_interval.is_zero(), "frame interval must be non-zero");
        Self {
            ring: ByteRing::new(capacity),
            frame_interval,
            first_pending_at: None,
        }
    }

    pub fn ingest_at(&mut self, bytes: &[u8], now: Duration) -> Result<usize, Watermark> {
        if !bytes.is_empty() && self.first_pending_at.is_none() {
            self.first_pending_at = Some(now);
        }
        self.ring.push(bytes)
    }

    pub fn should_flush(&self, now: Duration) -> bool {
        if self.ring.is_empty() {
            return false;
        }

        if self.ring.remaining_capacity() == 0 {
            return true;
        }

        self.first_pending_at
            .map(|started| now.saturating_sub(started) >= self.frame_interval)
            .unwrap_or(false)
    }

    pub fn flush(&mut self) -> Vec<u8> {
        self.first_pending_at = None;
        self.ring.drain_all()
    }

    pub fn pending_len(&self) -> usize {
        self.ring.len()
    }
}

/// Visible terminal dimensions used by PTY, parser, and renderer boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub rows: usize,
    pub cols: usize,
}

impl TerminalSize {
    pub fn new(rows: usize, cols: usize) -> anyhow::Result<Self> {
        if rows == 0 || cols == 0 {
            anyhow::bail!("terminal size rows and cols must be non-zero");
        }

        Ok(Self { rows, cols })
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

impl From<TerminalSize> for PtySize {
    fn from(size: TerminalSize) -> Self {
        Self {
            rows: size.rows as u16,
            cols: size.cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

impl TryFrom<PtySize> for TerminalSize {
    type Error = anyhow::Error;

    fn try_from(size: PtySize) -> Result<Self, Self::Error> {
        Self::new(size.rows as usize, size.cols as usize)
    }
}

/// Snapshot of the Rust-owned terminal grid for renderers and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSnapshot {
    size: TerminalSize,
    lines: Vec<String>,
}

impl TerminalSnapshot {
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    pub fn line_text(&self, row: usize) -> String {
        self.lines
            .get(row)
            .map(|line| line.trim_end().to_owned())
            .unwrap_or_default()
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

/// VT parser boundary backed by Alacritty's terminal core.
pub struct TerminalParser {
    size: TerminalSize,
    parser: VteProcessor,
    term: Term<VoidListener>,
}

impl TerminalParser {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            parser: VteProcessor::new(),
            term: Term::new(TermConfig::default(), &size, VoidListener),
        }
    }

    pub fn advance_bytes(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, size: TerminalSize) {
        self.term.resize(size);
        self.size = size;
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let mut lines = vec![String::new(); self.size.rows];

        for indexed in self.term.grid().display_iter() {
            let row = indexed.point.line.0 + self.term.grid().display_offset() as i32;
            if row < 0 {
                continue;
            }

            let row = row as usize;
            if row >= self.size.rows {
                continue;
            }

            lines[row].push(indexed.cell.c);
        }

        TerminalSnapshot {
            size: self.size,
            lines,
        }
    }
}

/// Live PTY read coalescing configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtyReadConfig {
    pub ring_capacity: usize,
    pub frame_interval: Duration,
}

impl PtyReadConfig {
    pub fn new(ring_capacity: usize, frame_interval: Duration) -> anyhow::Result<Self> {
        if ring_capacity == 0 {
            anyhow::bail!("PTY read ring capacity must be non-zero");
        }
        if frame_interval.is_zero() {
            anyhow::bail!("PTY read frame interval must be non-zero");
        }

        Ok(Self {
            ring_capacity,
            frame_interval,
        })
    }
}

impl Default for PtyReadConfig {
    fn default() -> Self {
        Self {
            ring_capacity: 1024 * 1024,
            frame_interval: Duration::from_millis(8),
        }
    }
}

/// A thin local PTY session wrapper for the slice-1 data plane.
///
/// It exposes PTY bytes, not parsed cells, so the same reader path can feed the coalescer today
/// and an `alacritty_terminal` parser in the next slice.
pub struct LocalPty {
    master: Box<dyn MasterPty>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output_rx: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    read_config: PtyReadConfig,
}

impl LocalPty {
    pub fn spawn(program: &str, args: &[&str]) -> anyhow::Result<Self> {
        Self::spawn_with_read_config(program, args, PtyReadConfig::default())
    }

    pub fn spawn_with_read_config(
        program: &str,
        args: &[&str],
        read_config: PtyReadConfig,
    ) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize::default())?;

        let mut cmd = CommandBuilder::new(program);
        for arg in args {
            cmd.arg(arg);
        }

        let child = pair.slave.spawn_command(cmd)?;
        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;
        let (output_tx, output_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut chunk = [0_u8; 8192];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx.send(Ok(chunk[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = output_tx.send(Err(err));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            master: pair.master,
            child,
            writer,
            output_rx,
            read_config,
        })
    }

    pub fn read_available(&mut self, timeout: Duration) -> std::io::Result<Vec<u8>> {
        Ok(self.read_coalesced(timeout)?.concat())
    }

    pub fn read_coalesced(&mut self, timeout: Duration) -> std::io::Result<Vec<Vec<u8>>> {
        let started = Instant::now();
        let deadline = started + timeout;
        let mut coalescer = Coalescer::new(
            self.read_config.ring_capacity,
            self.read_config.frame_interval,
        );
        let mut frames = Vec::new();

        loop {
            let now = Instant::now();
            if now >= deadline {
                Self::flush_pending(&mut coalescer, &mut frames);
                return Ok(frames);
            }

            match self.output_rx.recv_timeout(deadline - now) {
                Ok(Ok(chunk)) => {
                    Self::ingest_coalesced(&mut coalescer, &mut frames, &chunk, started.elapsed())?;
                    if coalescer.should_flush(started.elapsed()) {
                        Self::flush_pending(&mut coalescer, &mut frames);
                    }
                }
                Ok(Err(err)) => return Err(err),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    Self::flush_pending(&mut coalescer, &mut frames);
                    return Ok(frames);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    Self::flush_pending(&mut coalescer, &mut frames);
                    return Ok(frames);
                }
            }
        }
    }

    fn ingest_coalesced(
        coalescer: &mut Coalescer,
        frames: &mut Vec<Vec<u8>>,
        mut bytes: &[u8],
        now: Duration,
    ) -> std::io::Result<()> {
        while !bytes.is_empty() {
            match coalescer.ingest_at(bytes, now) {
                Ok(_) => return Ok(()),
                Err(Watermark::Full { accepted }) => {
                    bytes = &bytes[accepted..];
                    Self::flush_pending(coalescer, frames);

                    if accepted == 0 && bytes.len() > coalescer.ring.capacity() {
                        let capacity = coalescer.ring.capacity();
                        coalescer.ingest_at(&bytes[..capacity], now).map_err(|_| {
                            std::io::Error::other("coalescer rejected a capacity-sized chunk")
                        })?;
                        bytes = &bytes[capacity..];
                        Self::flush_pending(coalescer, frames);
                    }
                }
            }
        }
        Ok(())
    }

    fn flush_pending(coalescer: &mut Coalescer, frames: &mut Vec<Vec<u8>>) {
        let frame = coalescer.flush();
        if !frame.is_empty() {
            frames.push(frame);
        }
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }

    pub fn resize(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        self.master.resize(size.into())
    }

    pub fn size(&self) -> anyhow::Result<TerminalSize> {
        self.master.get_size()?.try_into()
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait()
    }
}

/// Throughput benchmark summary for the PTY data plane.
#[derive(Debug, Clone, PartialEq)]
pub struct ThroughputReport {
    pub bytes_read: usize,
    pub frames_read: usize,
    pub max_frame_bytes: usize,
    pub elapsed: Duration,
}

impl ThroughputReport {
    pub fn mib_per_second(&self) -> f64 {
        let seconds = self.elapsed.as_secs_f64();
        if seconds == 0.0 {
            return 0.0;
        }
        (self.bytes_read as f64 / 1024.0 / 1024.0) / seconds
    }
}

/// Run a command through LocalPty and measure coalesced output throughput.
pub fn measure_pty_throughput(
    program: &str,
    args: &[&str],
    read_config: PtyReadConfig,
    expected_bytes: usize,
    timeout: Duration,
) -> anyhow::Result<ThroughputReport> {
    if expected_bytes == 0 {
        anyhow::bail!("expected bytes must be non-zero");
    }
    if timeout.is_zero() {
        anyhow::bail!("benchmark timeout must be non-zero");
    }

    let mut pty = LocalPty::spawn_with_read_config(program, args, read_config)?;
    let started = Instant::now();
    let frames = pty.read_coalesced(timeout)?;
    let elapsed = started.elapsed();
    let bytes_read = frames.iter().map(Vec::len).sum();

    if bytes_read < expected_bytes {
        anyhow::bail!(
            "benchmark read {bytes_read} bytes before timeout/disconnect, expected at least {expected_bytes}"
        );
    }

    Ok(ThroughputReport {
        bytes_read,
        frames_read: frames.len(),
        max_frame_bytes: frames.iter().map(Vec::len).max().unwrap_or(0),
        elapsed,
    })
}

/// Stable identifier for terminal sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(u64);

impl From<u64> for SessionId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl SessionId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// One PTY plus its Rust-owned VT parser/grid state.
pub struct TerminalSession {
    id: SessionId,
    pty: LocalPty,
    parser: TerminalParser,
}

impl TerminalSession {
    pub fn spawn(
        id: SessionId,
        program: &str,
        args: &[&str],
        size: TerminalSize,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            id,
            pty: LocalPty::spawn(program, args)?,
            parser: TerminalParser::new(size),
        })
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn drain_output(&mut self, timeout: Duration) -> anyhow::Result<Vec<u8>> {
        let output = self.pty.read_available(timeout)?;
        self.parser.advance_bytes(&output);
        Ok(output)
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        Ok(self.pty.write_input(bytes)?)
    }

    pub fn resize(&mut self, size: TerminalSize) -> anyhow::Result<()> {
        self.pty.resize(size)?;
        self.parser.resize(size);
        Ok(())
    }

    pub fn size(&self) -> TerminalSize {
        self.parser.snapshot().size()
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        self.parser.snapshot()
    }

    pub fn wait(&mut self) -> anyhow::Result<ExitStatus> {
        Ok(self.pty.wait()?)
    }
}

/// Owns multiple terminal sessions and routes control/data-plane calls by id.
pub struct SessionManager {
    size: TerminalSize,
    next_id: u64,
    sessions: BTreeMap<SessionId, TerminalSession>,
}

impl SessionManager {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            next_id: 1,
            sessions: BTreeMap::new(),
        }
    }

    pub fn spawn(&mut self, program: &str, args: &[&str]) -> anyhow::Result<SessionId> {
        let id = SessionId(self.next_id);
        self.next_id += 1;

        let session = TerminalSession::spawn(id, program, args, self.size)?;
        self.sessions.insert(id, session);
        Ok(id)
    }

    pub fn session_ids(&self) -> Vec<SessionId> {
        self.sessions.keys().copied().collect()
    }

    pub fn drain_output(&mut self, id: SessionId, timeout: Duration) -> anyhow::Result<Vec<u8>> {
        self.session_mut(id)?.drain_output(timeout)
    }

    pub fn write_input(&mut self, id: SessionId, bytes: &[u8]) -> anyhow::Result<()> {
        self.session_mut(id)?.write_input(bytes)
    }

    pub fn resize(&mut self, id: SessionId, size: TerminalSize) -> anyhow::Result<()> {
        self.session_mut(id)?.resize(size)
    }

    pub fn snapshot(&self, id: SessionId) -> anyhow::Result<TerminalSnapshot> {
        Ok(self.session(id)?.snapshot())
    }

    pub fn session(&self, id: SessionId) -> anyhow::Result<&TerminalSession> {
        self.sessions
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("terminal session {} not found", id.get()))
    }

    pub fn session_mut(&mut self, id: SessionId) -> anyhow::Result<&mut TerminalSession> {
        self.sessions
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("terminal session {} not found", id.get()))
    }
}

/// Spill sink for scrollback lines beyond the in-memory visible cap.
pub trait ScrollbackSpill: std::fmt::Debug {
    fn append_line(&mut self, line: String) -> anyhow::Result<()>;
    fn len(&self) -> usize;
    fn lines(&self) -> Vec<String>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Deterministic in-memory spill sink used by tests and early slice-1 builds.
#[derive(Debug, Default, Clone)]
pub struct InMemorySpill {
    lines: Vec<String>,
}

impl ScrollbackSpill for InMemorySpill {
    fn append_line(&mut self, line: String) -> anyhow::Result<()> {
        self.lines.push(line);
        Ok(())
    }

    fn len(&self) -> usize {
        self.lines.len()
    }

    fn lines(&self) -> Vec<String> {
        self.lines.clone()
    }
}

/// Fixed-memory scrollback model.
///
/// Keeps only the visible cap in memory and delegates overflow to a spill sink. The default sink is
/// in-memory for deterministic tests; SQLite/mmap can replace it without changing this contract.
#[derive(Debug)]
pub struct Scrollback<S: ScrollbackSpill = InMemorySpill> {
    max_visible_lines: usize,
    visible: Vec<String>,
    spill: S,
    total: usize,
}

impl Scrollback<InMemorySpill> {
    pub fn new(max_visible_lines: usize) -> Self {
        Self::with_spill(max_visible_lines, InMemorySpill::default())
    }
}

impl<S: ScrollbackSpill> Scrollback<S> {
    pub fn with_spill(max_visible_lines: usize, spill: S) -> Self {
        assert!(
            max_visible_lines > 0,
            "scrollback visible line cap must be non-zero"
        );
        Self {
            max_visible_lines,
            visible: Vec::with_capacity(max_visible_lines),
            spill,
            total: 0,
        }
    }

    pub fn push_line(&mut self, line: impl Into<String>) -> anyhow::Result<()> {
        if self.visible.len() == self.max_visible_lines {
            self.spill.append_line(self.visible[0].clone())?;
            self.visible.remove(0);
        }
        self.visible.push(line.into());
        self.total += 1;
        Ok(())
    }

    pub fn visible_lines(&self) -> &[String] {
        &self.visible
    }

    pub fn spill_len(&self) -> usize {
        self.spill.len()
    }

    pub fn spilled_lines(&self) -> Vec<String> {
        self.spill.lines()
    }

    pub fn total_lines(&self) -> usize {
        self.total
    }
}
