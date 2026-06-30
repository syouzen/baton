use baton_core::{
    measure_pty_throughput, LocalPty, PtyReadConfig, SessionId, SessionManager, TerminalSize,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::ipc::Channel;

const DEFAULT_ROWS: usize = 24;
const DEFAULT_COLS: usize = 80;
const DEFAULT_PROGRAM: &str = "/bin/sh";

#[derive(Clone)]
pub struct AppState {
    manager: Arc<Mutex<SessionManager>>,
}

impl AppState {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            manager: Arc::new(Mutex::new(SessionManager::new(size))),
        }
    }

    #[cfg(test)]
    pub fn default_for_tests() -> Result<Self, String> {
        Ok(Self::new(command_size(DEFAULT_ROWS, DEFAULT_COLS)?))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(TerminalSize::new(DEFAULT_ROWS, DEFAULT_COLS).expect("default terminal size"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: u64,
    pub rows: usize,
    pub cols: usize,
}

impl From<(SessionId, TerminalSize)> for SessionView {
    fn from((id, size): (SessionId, TerminalSize)) -> Self {
        Self {
            id: id.get(),
            rows: size.rows,
            cols: size.cols,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slice1MeasurementReport {
    pub bytes_read: usize,
    pub frames_read: usize,
    pub max_frame_bytes: usize,
    pub elapsed_ms: u128,
    pub throughput_mib_per_second: f64,
    pub input_round_trip_ms: u128,
    pub resize_latency_micros: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSnapshotView {
    pub id: u64,
    pub rows: usize,
    pub cols: usize,
    pub lines: Vec<String>,
}

#[tauri::command]
pub fn create_session(
    state: tauri::State<'_, AppState>,
    program: Option<String>,
    args: Option<Vec<String>>,
    output: Channel<Vec<u8>>,
) -> Result<SessionView, String> {
    create_session_with_output_channel(&state, program, args, output)
}

#[tauri::command]
pub fn write_session(
    state: tauri::State<'_, AppState>,
    session_id: u64,
    bytes: Vec<u8>,
) -> Result<(), String> {
    write_session_for_state(&state, session_id, bytes)
}

#[tauri::command]
pub fn resize_session(
    state: tauri::State<'_, AppState>,
    session_id: u64,
    rows: usize,
    cols: usize,
    pixel_width: Option<u32>,
    pixel_height: Option<u32>,
) -> Result<SessionView, String> {
    resize_session_for_state(
        state.inner(),
        session_id,
        rows,
        cols,
        pixel_width,
        pixel_height,
    )
}

#[tauri::command]
pub fn kill_session(state: tauri::State<'_, AppState>, session_id: u64) -> Result<(), String> {
    kill_session_for_state(&state, session_id)
}

#[tauri::command]
pub fn list_sessions(state: tauri::State<'_, AppState>) -> Result<Vec<SessionView>, String> {
    list_sessions_for_state(&state)
}

#[tauri::command]
pub fn read_session(state: tauri::State<'_, AppState>, session_id: u64) -> Result<Vec<u8>, String> {
    read_session_for_state(&state, session_id)
}

#[tauri::command]
pub fn snapshot_session(
    state: tauri::State<'_, AppState>,
    session_id: u64,
) -> Result<TerminalSnapshotView, String> {
    snapshot_session_for_state(&state, session_id)
}

#[tauri::command]
pub fn run_baseline_measurement() -> Result<Slice1MeasurementReport, String> {
    run_baseline_measurement_for_config(1024 * 1024, Duration::from_secs(30))
}

#[cfg(test)]
pub fn create_session_for_state(
    state: &AppState,
    program: Option<String>,
    args: Option<Vec<String>>,
) -> Result<SessionView, String> {
    let program = program.unwrap_or_else(|| DEFAULT_PROGRAM.to_string());
    let args = args.unwrap_or_default();
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

    let mut manager = lock_manager(state)?;
    let id = manager
        .spawn(&program, &arg_refs)
        .map_err(to_command_error)?;
    let size = manager.session(id).map_err(to_command_error)?.size();
    Ok((id, size).into())
}

pub fn create_session_with_output_channel(
    state: &AppState,
    program: Option<String>,
    args: Option<Vec<String>>,
    output: Channel<Vec<u8>>,
) -> Result<SessionView, String> {
    let program = program.unwrap_or_else(|| DEFAULT_PROGRAM.to_string());
    let args = args.unwrap_or_default();
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

    let mut manager = lock_manager(state)?;
    let id = manager
        .spawn_with_output_handler(
            &program,
            &arg_refs,
            Some(Box::new(move |bytes| output.send(bytes.to_vec()).is_ok())),
        )
        .map_err(to_command_error)?;
    let size = manager.session(id).map_err(to_command_error)?.size();
    Ok((id, size).into())
}

pub fn write_session_for_state(
    state: &AppState,
    session_id: u64,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let mut manager = lock_manager(state)?;
    manager
        .write_input(SessionId::from(session_id), &bytes)
        .map_err(to_command_error)
}

pub fn resize_session_for_state(
    state: &AppState,
    session_id: u64,
    rows: usize,
    cols: usize,
    _pixel_width: Option<u32>,
    _pixel_height: Option<u32>,
) -> Result<SessionView, String> {
    let size = command_size(rows, cols)?;
    let mut manager = lock_manager(state)?;
    let id = SessionId::from(session_id);
    manager.resize(id, size).map_err(to_command_error)?;
    Ok((id, manager.session(id).map_err(to_command_error)?.size()).into())
}

pub fn kill_session_for_state(state: &AppState, session_id: u64) -> Result<(), String> {
    let mut manager = lock_manager(state)?;
    manager
        .kill(SessionId::from(session_id))
        .map_err(to_command_error)
}

pub fn list_sessions_for_state(state: &AppState) -> Result<Vec<SessionView>, String> {
    let manager = lock_manager(state)?;
    manager
        .session_ids()
        .into_iter()
        .map(|id| {
            manager
                .session(id)
                .map(|session| (id, session.size()).into())
                .map_err(to_command_error)
        })
        .collect()
}

pub fn read_session_for_state(state: &AppState, session_id: u64) -> Result<Vec<u8>, String> {
    let id = SessionId::from(session_id);
    let mut manager = lock_manager(state)?;
    manager
        .drain_output(id, Duration::from_millis(16))
        .map_err(to_command_error)
}

pub fn snapshot_session_for_state(
    state: &AppState,
    session_id: u64,
) -> Result<TerminalSnapshotView, String> {
    let id = SessionId::from(session_id);
    let manager = lock_manager(state)?;
    let snapshot = manager.snapshot(id).map_err(to_command_error)?;
    let size = snapshot.size();

    Ok(TerminalSnapshotView {
        id: session_id,
        rows: size.rows,
        cols: size.cols,
        lines: snapshot.lines().to_vec(),
    })
}

pub fn run_baseline_measurement_for_config(
    bytes: usize,
    timeout: Duration,
) -> Result<Slice1MeasurementReport, String> {
    let command =
        format!("python3 - <<'PY'\nimport sys\nsys.stdout.buffer.write(b'x' * {bytes})\nPY");
    let throughput = measure_pty_throughput(
        "/bin/sh",
        &["-lc", &command],
        PtyReadConfig::default(),
        bytes,
        timeout,
    )
    .map_err(to_command_error)?;

    let input_started = Instant::now();
    let mut echo = LocalPty::spawn("/bin/sh", &["-lc", "read line; printf '%s' \"$line\""])
        .map_err(to_command_error)?;
    echo.write_input(b"baton-latency\n")
        .map_err(to_command_error)?;
    let echoed = echo
        .read_available(Duration::from_secs(3))
        .map_err(to_command_error)?;
    let input_round_trip_ms = input_started.elapsed().as_millis().max(1);
    if !String::from_utf8_lossy(&echoed).contains("baton-latency") {
        return Err("input latency probe did not echo expected marker".to_string());
    }

    let mut resize_probe =
        LocalPty::spawn("/bin/sh", &["-lc", "sleep 1"]).map_err(to_command_error)?;
    let resize_started = Instant::now();
    resize_probe
        .resize(TerminalSize::new(30, 100).map_err(to_command_error)?)
        .map_err(to_command_error)?;
    let resize_latency_micros = resize_started.elapsed().as_micros().max(1);
    let _ = resize_probe.kill();

    Ok(Slice1MeasurementReport {
        bytes_read: throughput.bytes_read,
        frames_read: throughput.frames_read,
        max_frame_bytes: throughput.max_frame_bytes,
        elapsed_ms: throughput.elapsed.as_millis(),
        throughput_mib_per_second: throughput.mib_per_second(),
        input_round_trip_ms,
        resize_latency_micros,
    })
}

fn command_size(rows: usize, cols: usize) -> Result<TerminalSize, String> {
    TerminalSize::new(rows, cols).map_err(to_command_error)
}

fn lock_manager(state: &AppState) -> Result<std::sync::MutexGuard<'_, SessionManager>, String> {
    state
        .manager
        .lock()
        .map_err(|_| "terminal session manager lock poisoned".to_string())
}

fn to_command_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_surface_creates_lists_resizes_and_kills_sessions() {
        let state = AppState::default_for_tests().expect("test state");

        let created = create_session_for_state(
            &state,
            Some("/bin/sh".to_string()),
            Some(vec!["-lc".to_string(), "sleep 30".to_string()]),
        )
        .expect("session created");

        assert_eq!(created.id, 1);
        assert_eq!(created.rows, 24);
        assert_eq!(created.cols, 80);

        let listed = list_sessions_for_state(&state).expect("sessions listed");
        assert_eq!(listed, vec![created.clone()]);

        let resized = resize_session_for_state(&state, created.id, 40, 120, Some(960), Some(640))
            .expect("session resized");
        assert_eq!(resized.rows, 40);
        assert_eq!(resized.cols, 120);

        kill_session_for_state(&state, created.id).expect("session killed");
        assert!(list_sessions_for_state(&state)
            .expect("sessions listed")
            .is_empty());
    }

    #[test]
    fn command_surface_reports_invalid_session_ids() {
        let state = AppState::default_for_tests().expect("test state");

        let write_error = write_session_for_state(&state, 404, vec![b'a']).unwrap_err();
        assert!(write_error.contains("terminal session 404 not found"));

        let resize_error = resize_session_for_state(&state, 404, 24, 80, None, None).unwrap_err();
        assert!(resize_error.contains("terminal session 404 not found"));

        let kill_error = kill_session_for_state(&state, 404).unwrap_err();
        assert!(kill_error.contains("terminal session 404 not found"));
    }

    #[test]
    fn command_surface_rejects_zero_sized_resize() {
        let state = AppState::default_for_tests().expect("test state");

        let created = create_session_for_state(
            &state,
            Some("/bin/sh".to_string()),
            Some(vec!["-lc".to_string(), "sleep 30".to_string()]),
        )
        .expect("session created");

        let error = resize_session_for_state(&state, created.id, 0, 80, None, None).unwrap_err();
        assert!(error.contains("terminal size rows and cols must be non-zero"));

        kill_session_for_state(&state, created.id).expect("session killed");
    }

    #[test]
    fn baseline_measurement_reports_numeric_metrics() {
        let report = run_baseline_measurement_for_config(65_536, Duration::from_secs(3))
            .expect("baseline measurement");

        assert_eq!(report.bytes_read, 65_536);
        assert!(report.frames_read >= 1);
        assert!(report.max_frame_bytes > 0);
        assert!(report.elapsed_ms > 0);
        assert!(report.throughput_mib_per_second > 0.0);
        assert!(report.input_round_trip_ms > 0);
        assert!(report.resize_latency_micros > 0);
    }

    #[test]
    fn command_surface_snapshots_session_grid() {
        let state = AppState::default_for_tests().expect("test state");
        let created = create_session_for_state(
            &state,
            Some("/bin/sh".to_string()),
            Some(vec![
                "-lc".to_string(),
                "printf 'alpha\\nbeta\\n'".to_string(),
            ]),
        )
        .expect("session created");

        let mut snapshot = None;
        for _ in 0..10 {
            let _ = read_session_for_state(&state, created.id).expect("output drained");
            let candidate = snapshot_session_for_state(&state, created.id).expect("snapshot");
            if candidate.lines.iter().any(|line| line.contains("alpha")) {
                snapshot = Some(candidate);
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let snapshot = snapshot.expect("snapshot contains command output");

        assert_eq!(snapshot.id, created.id);
        assert_eq!(snapshot.rows, 24);
        assert_eq!(snapshot.cols, 80);
        assert!(snapshot.lines.iter().any(|line| line.contains("alpha")));
        assert!(snapshot.lines.iter().any(|line| line.contains("beta")));
    }

    #[test]
    fn command_surface_drains_coalesced_output() {
        let state = AppState::default_for_tests().expect("test state");
        let created = create_session_for_state(
            &state,
            Some("/bin/sh".to_string()),
            Some(vec!["-lc".to_string(), "printf baton-output".to_string()]),
        )
        .expect("session created");

        let output = read_session_for_state(&state, created.id).expect("session output");
        assert_eq!(String::from_utf8(output).unwrap(), "baton-output");
    }
}
