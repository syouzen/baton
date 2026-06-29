use baton_core::{LocalPty, PtyReadConfig};
use std::time::Duration;

#[test]
fn live_pty_reader_coalesces_output_without_dropping_bytes() {
    let config = PtyReadConfig::new(4, Duration::from_millis(50)).unwrap();
    let mut pty =
        LocalPty::spawn_with_read_config("/bin/sh", &["-lc", "printf abcdefghij"], config).unwrap();

    let frames = pty.read_coalesced(Duration::from_secs(2)).unwrap();
    let joined = frames.concat();

    assert_eq!(String::from_utf8_lossy(&joined), "abcdefghij");
    assert!(frames.iter().all(|frame| frame.len() <= 4));
    assert!(frames.len() >= 3);
}

#[test]
fn read_available_preserves_flattened_output_contract() {
    let config = PtyReadConfig::new(4, Duration::from_millis(50)).unwrap();
    let mut pty =
        LocalPty::spawn_with_read_config("/bin/sh", &["-lc", "printf flattened"], config).unwrap();

    let output = pty.read_available(Duration::from_secs(2)).unwrap();

    assert_eq!(String::from_utf8_lossy(&output), "flattened");
}
