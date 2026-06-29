use baton_core::{LocalPty, SessionManager, TerminalParser, TerminalSize};
use std::time::Duration;

#[test]
fn terminal_parser_resize_updates_snapshot_dimensions() {
    let mut parser = TerminalParser::new(TerminalSize::new(2, 4).unwrap());
    parser.advance_bytes(b"abcd\nefgh");

    parser.resize(TerminalSize::new(4, 8).unwrap());
    let snapshot = parser.snapshot();

    assert_eq!(snapshot.size(), TerminalSize::new(4, 8).unwrap());
    assert_eq!(snapshot.lines().len(), 4);
}

#[test]
fn local_pty_resize_updates_kernel_winsize() {
    let mut pty = LocalPty::spawn("/bin/sh", &["-lc", "stty size; sleep 0.1"]).unwrap();

    pty.resize(TerminalSize::new(33, 77).unwrap()).unwrap();
    let size = pty.size().unwrap();

    assert_eq!(size, TerminalSize::new(33, 77).unwrap());
}

#[test]
fn session_manager_resize_updates_pty_and_parser_viewport() {
    let mut manager = SessionManager::new(TerminalSize::new(5, 20).unwrap());
    let session_id = manager
        .spawn("/bin/sh", &["-lc", "printf resized"])
        .unwrap();

    manager
        .resize(session_id, TerminalSize::new(8, 30).unwrap())
        .unwrap();
    manager
        .drain_output(session_id, Duration::from_secs(2))
        .unwrap();

    let snapshot = manager.snapshot(session_id).unwrap();
    assert_eq!(snapshot.size(), TerminalSize::new(8, 30).unwrap());
    assert_eq!(
        manager.session(session_id).unwrap().size(),
        TerminalSize::new(8, 30).unwrap()
    );
}

#[test]
fn resizing_missing_session_returns_error() {
    let mut manager = SessionManager::new(TerminalSize::new(5, 20).unwrap());

    assert!(manager
        .resize(404.into(), TerminalSize::new(8, 30).unwrap())
        .is_err());
}
