use baton_core::{LocalPty, PtyReadConfig};
use std::sync::mpsc;
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

#[test]
fn output_handler_backpressure_stops_reader_until_consumer_drains_frames() {
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
    let target_bytes = 65_536usize;
    let command = format!(
        "python3 - <<'PY'\nimport sys\nsys.stdout.buffer.write(b'x' * {target_bytes})\nsys.stdout.flush()\nPY"
    );

    let mut pty = LocalPty::spawn_with_read_config_and_output_handler(
        "/bin/sh",
        &["-lc", &command],
        PtyReadConfig::new(4096, Duration::from_millis(50)).unwrap(),
        Some(Box::new(move |bytes| tx.send(bytes.to_vec()).is_ok())),
    )
    .unwrap();

    std::thread::sleep(Duration::from_millis(100));
    let first = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("one frame should be buffered");
    assert!(!first.is_empty() && first.len() <= 4096);
    assert!(
        matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "bounded handler channel must block the reader instead of draining PTY into unbounded memory"
    );

    let mut total = first.len();
    while total < target_bytes {
        total += rx
            .recv_timeout(Duration::from_secs(2))
            .expect("reader resumes when consumer drains")
            .len();
    }

    assert_eq!(total, target_bytes);
    let _ = pty.wait();
}
