use baton_core::{measure_pty_throughput, PtyReadConfig};
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let bytes = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100 * 1024 * 1024);

    let command =
        format!("python3 - <<'PY'\nimport sys\nsys.stdout.buffer.write(b'x' * {bytes})\nPY");
    let report = measure_pty_throughput(
        "/bin/sh",
        &["-lc", &command],
        PtyReadConfig::default(),
        bytes,
        Duration::from_secs(30),
    )?;

    println!("bytes_read={}", report.bytes_read);
    println!("frames_read={}", report.frames_read);
    println!("max_frame_bytes={}", report.max_frame_bytes);
    println!("elapsed_ms={}", report.elapsed.as_millis());
    println!("mib_per_second={:.2}", report.mib_per_second());

    Ok(())
}
