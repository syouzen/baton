# Slice-1 hot-path measurement record

Measured: 2026-06-30 15:06 KST on local macOS host.

## Gates

- PTY throughput target: >=100 MiB/s sustained with no drops.
- Input latency target: PTY round trip plus <=8 ms overhead. Current probe measures local PTY write-to-echo round trip for the Baton core path.
- Idle CPU target: ~0%, no polling.
- Cold start target: <300 ms. Current probe measures local PTY spawn to first output; full Tauri window cold-start still needs GUI-level instrumentation.

## Results

### 100 MiB PTY throughput (`cargo run --release --example throughput_bench -- <workload> 104857600`)

- `cat`: 104,857,600 bytes, 102,400 frames, max frame 1,024 bytes, 695-704 ms, 141.98-143.75 MiB/s.
- `waterfall`: 108,898,764 bytes, 106,347 frames, max frame 1,024 bytes, 681 ms, 152.45 MiB/s.
- `ansi`: 109,231,111 bytes, 106,672 frames, max frame 1,024 bytes, 770 ms, 135.27 MiB/s.
- `unicode`: 109,416,626 bytes, 106,853 frames, max frame 1,024 bytes, 683 ms, 152.66 MiB/s.
- `scroll`: 111,025,694 bytes, 108,424 frames, max frame 1,024 bytes, 758 ms, 139.60 MiB/s.

### Input / idle / cold-start probes (`/usr/bin/time -l cargo run --release --example slice1_measure`)

- Throughput probe in combined measurement: 104,857,600 bytes in 695 ms = 143.75 MiB/s.
- Input echo round trip: 7.154 ms, echo marker verified.
- Idle read probe: 519 ms wait, 0 output bytes.
- Local PTY cold start to first output: 33.105 ms, marker verified.
- Resize latency: 12 µs.

### Idle-only process probe (`cargo build --release --example idle_probe && /usr/bin/time -l target/release/examples/idle_probe`)

- Idle wait: 2,088 ms, 0 output bytes.
- Process time: 2.31 real / 0.01 user / 0.02 sys seconds.
- This is low but not a perfect GUI idle CPU measurement; use app-level sampling before declaring full idle-CPU acceptance.

## Decision note

The core PTY data path clears the 100 MiB/s throughput target in release-mode probes after moving output streaming off JSON event IPC and onto a Tauri channel path. Do not start Slice 2/native renderer from these numbers alone: the GUI/webview renderer frame-drop and full Tauri cold-start measurements still need app-level instrumentation before a formal go/no-go.
