use baton_core::{
    measure_pty_throughput, run_throughput_workload, PtyReadConfig, ThroughputWorkload,
};
use std::time::Duration;

#[test]
fn throughput_workloads_are_named_and_machine_readable() {
    let names = ThroughputWorkload::names();
    assert_eq!(names, ["cat", "waterfall", "ansi", "unicode", "scroll"]);

    let workload = ThroughputWorkload::from_name("ansi").expect("ansi workload");
    assert_eq!(workload.name(), "ansi");
    assert!(workload.expected_bytes(4096) >= 4096);
    assert!(workload.shell_command(4096).contains("\\x1b[31m"));
}

#[test]
fn throughput_workload_runner_reports_selected_workload() {
    let report = run_throughput_workload(
        ThroughputWorkload::Unicode,
        4096,
        PtyReadConfig::new(2048, Duration::from_millis(8)).unwrap(),
        Duration::from_secs(3),
    )
    .unwrap();

    assert_eq!(report.workload, "unicode");
    assert!(report.bytes_read >= 4096);
    assert!(report.frames_read >= 1);
    assert!(report.mib_per_second > 0.0);
}

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
