use baton_core::{measure_pty_throughput, LocalPty, PtyReadConfig, TerminalSize};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let bytes = 100 * 1024 * 1024;
    let command =
        format!("python3 - <<'PY'\nimport sys\nsys.stdout.buffer.write(b'x' * {bytes})\nPY");
    let throughput = measure_pty_throughput(
        "/bin/sh",
        &["-lc", &command],
        PtyReadConfig::default(),
        bytes,
        Duration::from_secs(30),
    )?;
    println!(
        "throughput_mib_per_second={:.2}",
        throughput.mib_per_second()
    );
    println!("bytes_read={}", throughput.bytes_read);
    println!("frames_read={}", throughput.frames_read);
    println!("max_frame_bytes={}", throughput.max_frame_bytes);
    println!("throughput_elapsed_ms={}", throughput.elapsed.as_millis());

    let mut echo = LocalPty::spawn("/bin/sh", &["-lc", "read line; printf '%s' \"$line\""])?;
    let input_started = Instant::now();
    echo.write_input(b"baton-latency\n")?;
    let echoed = echo.read_available(Duration::from_secs(3))?;
    println!(
        "input_round_trip_ms={}",
        input_started.elapsed().as_secs_f64() * 1000.0
    );
    println!(
        "input_echo_ok={}",
        String::from_utf8_lossy(&echoed).contains("baton-latency")
    );

    let mut idle = LocalPty::spawn("/bin/sh", &["-lc", "sleep 1"])?;
    let idle_started = Instant::now();
    let idle_output = idle.read_available(Duration::from_millis(500))?;
    println!("idle_probe_ms={}", idle_started.elapsed().as_millis());
    println!("idle_output_bytes={}", idle_output.len());
    let _ = idle.kill();

    let cold_started = Instant::now();
    let mut cold = LocalPty::spawn("/bin/sh", &["-lc", "printf ready"])?;
    let cold_output = cold.read_available(Duration::from_secs(3))?;
    println!(
        "local_pty_cold_start_to_first_output_ms={}",
        cold_started.elapsed().as_secs_f64() * 1000.0
    );
    println!(
        "cold_output_ok={}",
        String::from_utf8_lossy(&cold_output).contains("ready")
    );

    let mut resize_probe = LocalPty::spawn("/bin/sh", &["-lc", "sleep 1"])?;
    let resize_started = Instant::now();
    resize_probe.resize(TerminalSize::new(30, 100)?)?;
    println!(
        "resize_latency_micros={}",
        resize_started.elapsed().as_micros()
    );
    let _ = resize_probe.kill();
    Ok(())
}
