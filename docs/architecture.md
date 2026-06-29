# Baton architecture

Baton is built around one rule: keep terminal throughput out of the webview hot path.

## Current slice-1 pipeline

```text
PTY process
  │ bytes
  ▼
LocalPty reader thread
  │ chunks
  ▼
ByteRing + Coalescer
  │ frame-cadenced byte batches
  ▼
TerminalParser
  │ alacritty_terminal Term/Grid snapshot
  ▼
Renderer boundary
```

The current crate implements the data-plane contracts that the Tauri shell and renderer will consume:

- `LocalPty`: owns the PTY master, reader thread, writer, resize control, and child wait handle.
- `PtyReadConfig`: controls ring capacity and coalescing frame interval.
- `ByteRing`: accepts bytes up to a hard cap and reports backpressure instead of dropping data.
- `Coalescer`: flushes byte batches by frame interval or capacity.
- `TerminalParser`: owns the Rust VT parser/grid boundary through `alacritty_terminal`.
- `TerminalSession`: combines one PTY and one parser/grid.
- `SessionManager`: routes create, read, write, resize, snapshot, and wait calls by `SessionId`.
- `Scrollback`: keeps only visible lines in memory and delegates overflow to `ScrollbackSpill`.
- `measure_pty_throughput`: smoke benchmark helper for coalesced PTY output.

## Hot path vs control plane

Hot path responsibilities are latency- and throughput-sensitive:

1. PTY read/write
2. byte coalescing and backpressure
3. VT parsing and grid mutation
4. renderer damage/snapshot boundary
5. visible scrollback memory limits

Control plane responsibilities are low-frequency and may cross JSON/Tauri IPC:

- create/kill/focus terminal sessions
- resize terminal sessions
- scroll/selection requests
- tab/chrome state
- orchestration metadata
- settings and persistence commands

Do not move raw PTY byte streaming, VT parsing, or the terminal grid into JavaScript unless it is the explicit slice-1 xterm.js WebGL escape hatch. The long-term hot-path contract is Rust-owned PTY/parser/grid with the webview limited to chrome.

## Slice strategy

### Slice 1: working terminal and measurement

Slice 1 prioritizes a small working product and measurable throughput:

- Tauri v2 shell
- xterm.js WebGL renderer as an escape hatch
- coalesced PTY bytes from Rust
- command APIs for create/write/resize/kill
- benchmark and smoke commands

This slice may send coalesced bytes to xterm.js because it is the fastest way to get a usable renderer and baseline measurements.

### Slice 2: native GPU only if needed

Native GPU composition is intentionally not the default next step. It should start only if slice-1 measurements miss the target:

- Rust keeps parser/grid ownership.
- Renderer becomes native `wgpu` + glyph atlas + damage tracking.
- Webview remains chrome only, with the terminal cell region composed separately.

This avoids spending complexity budget on native child-surface composition before the benchmark says it is required.

## Backpressure contract

The data plane must preserve bytes. When a ring fills, callers get `Watermark::Full { accepted }`; they must flush or stop reading rather than drop output. This lets the OS pipe buffer slow producers naturally.

## Scrollback contract

`Scrollback` has a fixed visible cap. Lines that exceed the cap go to a `ScrollbackSpill` implementation. The default `InMemorySpill` is deterministic for tests; future SQLite or mmap spill storage should preserve the same public behavior.

If a spill append fails, the visible state must remain intact and the caller receives an error.

## Local verification

Run the full local gate before every PR:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all -- --nocapture
cargo check
```

Run the throughput smoke when touching PTY/coalescing/parser paths:

```bash
cargo run --example throughput_bench -- 1048576
```

The smoke command reports bytes read, frames read, largest frame, elapsed time, and MiB/s. It is a regression signal, not a final performance claim.
