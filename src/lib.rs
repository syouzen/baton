//! Baton terminal core primitives.
//!
//! This crate intentionally starts with the data-plane rules from the spec:
//! bounded byte buffers, frame-paced coalescing, and fixed-memory scrollback.
//! UI/chrome and native GPU composition can evolve without changing these hot-path contracts.

use std::collections::VecDeque;
use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::vte::ansi::Processor as VteProcessor;
use portable_pty::{native_pty_system, Child, CommandBuilder, ExitStatus, PtySize};

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

/// A thin local PTY session wrapper for the slice-1 data plane.
///
/// It exposes PTY bytes, not parsed cells, so the same reader path can feed the coalescer today
/// and an `alacritty_terminal` parser in the next slice.
pub struct LocalPty {
    child: Box<dyn Child + Send + Sync>,
    output_rx: mpsc::Receiver<std::io::Result<Vec<u8>>>,
}

impl LocalPty {
    pub fn spawn(program: &str, args: &[&str]) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize::default())?;

        let mut cmd = CommandBuilder::new(program);
        for arg in args {
            cmd.arg(arg);
        }

        let child = pair.slave.spawn_command(cmd)?;
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

        Ok(Self { child, output_rx })
    }

    pub fn read_available(&mut self, timeout: Duration) -> std::io::Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut out = Vec::new();

        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(out);
            }

            match self.output_rx.recv_timeout(deadline - now) {
                Ok(Ok(chunk)) => out.extend_from_slice(&chunk),
                Ok(Err(err)) => return Err(err),
                Err(mpsc::RecvTimeoutError::Timeout) => return Ok(out),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(out),
            }
        }
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait()
    }
}

/// Fixed-memory scrollback model.
///
/// MVP keeps spilled lines in-memory so behavior is deterministic and testable; replacing the spill
/// vector with SQLite/mmap storage preserves this public contract.
#[derive(Debug, Clone)]
pub struct Scrollback {
    max_visible_lines: usize,
    visible: Vec<String>,
    spilled: Vec<String>,
    total: usize,
}

impl Scrollback {
    pub fn new(max_visible_lines: usize) -> Self {
        assert!(
            max_visible_lines > 0,
            "scrollback visible line cap must be non-zero"
        );
        Self {
            max_visible_lines,
            visible: Vec::with_capacity(max_visible_lines),
            spilled: Vec::new(),
            total: 0,
        }
    }

    pub fn push_line(&mut self, line: impl Into<String>) {
        if self.visible.len() == self.max_visible_lines {
            let oldest = self.visible.remove(0);
            self.spilled.push(oldest);
        }
        self.visible.push(line.into());
        self.total += 1;
    }

    pub fn visible_lines(&self) -> &[String] {
        &self.visible
    }

    pub fn spilled_lines(&self) -> &[String] {
        &self.spilled
    }

    pub fn total_lines(&self) -> usize {
        self.total
    }
}
