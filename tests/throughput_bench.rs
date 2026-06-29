use baton_core::{measure_pty_throughput, PtyReadConfig};
use std::time::Duration;

#[test]
fn throughput_harness_reports_bytes_frames_and_rate() {
    let report = measure_pty_throughput(
        "/bin/sh",
        &[
            "-lc",
            "python3 - <<'PY'\nimport sys\nsys.stdout.buffer.write(b'x' * 65536)\nPY",
        ],
        PtyReadConfig::new(8192, Duration::from_millis(8)).unwrap(),
        65_536,
        Duration::from_secs(3),
    )
    .unwrap();

    assert_eq!(report.bytes_read, 65_536);
    assert!(report.frames_read >= 1);
    assert!(report.max_frame_bytes <= 8192);
    assert!(report.elapsed > Duration::ZERO);
    assert!(report.mib_per_second() > 0.0);
}

#[test]
fn throughput_harness_rejects_zero_expected_bytes() {
    let err = measure_pty_throughput(
        "/bin/echo",
        &["ignored"],
        PtyReadConfig::default(),
        0,
        Duration::from_secs(1),
    )
    .unwrap_err();

    assert!(err.to_string().contains("expected bytes must be non-zero"));
}
