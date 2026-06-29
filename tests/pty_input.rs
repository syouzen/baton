use baton_core::{SessionManager, TerminalSize};
use std::time::Duration;

#[test]
fn session_manager_writes_input_to_interactive_pty() {
    let mut manager = SessionManager::new(TerminalSize::new(5, 40).unwrap());
    let session_id = manager.spawn("/bin/cat", &[]).unwrap();

    manager.write_input(session_id, b"baton-input\n").unwrap();
    manager
        .drain_output(session_id, Duration::from_secs(2))
        .unwrap();

    let snapshot = manager.snapshot(session_id).unwrap();
    assert_eq!(snapshot.line_text(0), "baton-input");
}

#[test]
fn writing_to_missing_session_returns_error() {
    let mut manager = SessionManager::new(TerminalSize::new(5, 40).unwrap());

    assert!(manager.write_input(404.into(), b"lost").is_err());
}
