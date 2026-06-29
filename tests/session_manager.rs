use baton_core::{SessionManager, TerminalSize};
use std::time::Duration;

#[test]
fn session_manager_creates_unique_sessions_and_lists_them() {
    let mut manager = SessionManager::new(TerminalSize::new(5, 20).unwrap());

    let first = manager.spawn("/bin/sh", &["-lc", "printf first"]).unwrap();
    let second = manager.spawn("/bin/sh", &["-lc", "printf second"]).unwrap();

    assert_ne!(first, second);
    assert_eq!(manager.session_ids(), vec![first, second]);
}

#[test]
fn terminal_session_drains_pty_output_into_parser_snapshot() {
    let mut manager = SessionManager::new(TerminalSize::new(5, 20).unwrap());
    let session_id = manager
        .spawn("/bin/sh", &["-lc", "printf baton-session"])
        .unwrap();

    manager
        .drain_output(session_id, Duration::from_secs(2))
        .unwrap();
    let snapshot = manager.snapshot(session_id).unwrap();

    assert_eq!(snapshot.line_text(0), "baton-session");
}

#[test]
fn session_manager_reports_missing_sessions() {
    let manager = SessionManager::new(TerminalSize::new(5, 20).unwrap());

    assert!(manager.snapshot(999.into()).is_err());
}
