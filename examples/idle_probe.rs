use baton_core::LocalPty;
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let mut idle = LocalPty::spawn("/bin/sh", &["-lc", "sleep 2"])?;
    let started = Instant::now();
    let output = idle.read_available(Duration::from_secs(2))?;
    println!("idle_wait_ms={}", started.elapsed().as_millis());
    println!("idle_output_bytes={}", output.len());
    let _ = idle.kill();
    Ok(())
}
