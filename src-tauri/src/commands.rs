use baton_core::{SessionId, SessionManager, TerminalSize};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::Emitter;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputEvent {
    pub session_id: u64,
    pub bytes: Vec<u8>,
}

impl From<(SessionId, Vec<u8>)> for TerminalOutputEvent {
    fn from((id, bytes): (SessionId, Vec<u8>)) -> Self {
        Self {
            session_id: id.get(),
            bytes,
        }
    }
}

#[tauri::command]
pub fn create_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    program: Option<String>,
    args: Option<Vec<String>>,
) -> Result<SessionView, String> {
    let session = create_session_for_state(&state, program, args)?;
    spawn_output_loop(app, state.inner().clone(), session.id);
    Ok(session)
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
pub fn read_session(
    state: tauri::State<'_, AppState>,
    session_id: u64,
) -> Result<TerminalOutputEvent, String> {
    read_session_for_state(&state, session_id)
}

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

pub fn read_session_for_state(
    state: &AppState,
    session_id: u64,
) -> Result<TerminalOutputEvent, String> {
    let id = SessionId::from(session_id);
    let mut manager = lock_manager(state)?;
    let bytes = manager
        .drain_output(id, Duration::from_millis(16))
        .map_err(to_command_error)?;
    Ok((id, bytes).into())
}

fn spawn_output_loop(app: tauri::AppHandle, state: AppState, session_id: u64) {
    thread::spawn(move || loop {
        match read_session_for_state(&state, session_id) {
            Ok(output) if output.bytes.is_empty() => thread::sleep(Duration::from_millis(8)),
            Ok(output) => {
                let _ = app.emit("terminal-output", output);
            }
            Err(_) => break,
        }
    });
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
    fn command_surface_drains_coalesced_output() {
        let state = AppState::default_for_tests().expect("test state");
        let created = create_session_for_state(
            &state,
            Some("/bin/sh".to_string()),
            Some(vec!["-lc".to_string(), "printf baton-output".to_string()]),
        )
        .expect("session created");

        let output = read_session_for_state(&state, created.id).expect("session output");
        assert_eq!(output.session_id, created.id);
        assert_eq!(String::from_utf8(output.bytes).unwrap(), "baton-output");
    }
}
