use baton_core::{run_throughput_workload, PtyReadConfig, ThroughputWorkload};
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let workload_name = std::env::args().nth(1).unwrap_or_else(|| "cat".to_string());
    let workload = ThroughputWorkload::from_name(&workload_name).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown workload '{workload_name}', expected one of: {}",
            ThroughputWorkload::names().join(", ")
        )
    })?;
    let bytes = std::env::args()
        .nth(2)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100 * 1024 * 1024);

    let report = run_throughput_workload(
        workload,
        bytes,
        PtyReadConfig::default(),
        Duration::from_secs(30),
    )?;

    println!("workload={}", report.workload);
    println!("bytes_read={}", report.bytes_read);
    println!("frames_read={}", report.frames_read);
    println!("max_frame_bytes={}", report.max_frame_bytes);
    println!("elapsed_ms={}", report.elapsed.as_millis());
    println!("mib_per_second={:.2}", report.mib_per_second);

    Ok(())
}
