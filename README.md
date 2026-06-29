# baton

Spec-driven terminal core prototype for a high-throughput agent terminal.

Baton's core rule is simple: keep terminal throughput out of the webview hot path. Rust owns PTY I/O, byte coalescing, backpressure, VT parsing, terminal grid state, and fixed-memory scrollback contracts. The Tauri/webview layer should provide chrome and low-frequency control APIs.

## Implemented slice-1 core

- bounded PTY byte ring with explicit backpressure (`ByteRing`)
- live PTY reader coalescing with configurable frame/capacity policy (`PtyReadConfig`)
- local PTY spawn/read/write/resize/wait wrapper (`LocalPty`)
- Alacritty-backed VT parser boundary and visible grid snapshot (`TerminalParser`)
- multi-session routing model (`SessionId`, `TerminalSession`, `SessionManager`)
- fixed-visible-line scrollback contract with pluggable spill sink (`ScrollbackSpill`)
- throughput benchmark harness for coalesced PTY output (`measure_pty_throughput`)

## Implemented slice-1 shell

- Tauri v2 desktop app scaffold (`src-tauri/`)
- Vite + TypeScript frontend shell (`index.html`, `src/main.ts`, `src/styles.css`)
- one main window titled `baton`
- minimal chrome bar, session rail, and reserved terminal pane placeholder
- Tauri command API for session create/write/resize/kill/list control plane
- no IDE side features; terminal hot-path remains in Rust core

## Architecture

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

Hot path:

1. PTY read/write
2. byte coalescing and backpressure
3. VT parsing and grid mutation
4. renderer snapshot/damage boundary
5. fixed visible scrollback with spill storage

Control plane:

- create/kill/focus terminal sessions
- resize terminal sessions
- scroll/selection requests
- tab/chrome state
- orchestration metadata
- settings and persistence commands

See [`docs/architecture.md`](docs/architecture.md) for the detailed boundary contract.

## Verify

Run the full local gate before every PR:

```bash
npm install
npm test
npm run build
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --all -- --nocapture
cargo check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml -- --nocapture
npm run tauri -- build --debug
```

For interactive shell verification:

```bash
npm run tauri -- dev
```

Run the PTY throughput smoke when touching PTY/coalescing/parser paths:

```bash
cargo run --example throughput_bench -- 1048576
```

Example smoke output shape:

```text
bytes_read=1048576
frames_read=<n>
max_frame_bytes=<n>
elapsed_ms=<n>
mib_per_second=<n>
```

This smoke is a regression signal, not a final performance claim. Larger benchmark targets should be compared only after the slice-1 Tauri/xterm.js path exists.

## Branch workflow

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the repository workflow:

- feature branches target `develop`
- green PRs are squash-merged and branches are deleted
- release/stabilization PRs promote `develop` to protected `main`
- protected `main` requires PR-based changes and the `Rust core` CI check

## Slice strategy

1. **Slice 1:** build the Tauri v2 shell, command API, and xterm.js WebGL renderer as the escape-hatch renderer. This gets a working terminal and measurable baseline quickly.
2. **Measure:** use vtebench-style workloads, waterfall output, idle CPU, input latency, and the throughput harness.
3. **Slice 2 only if needed:** native GPU renderer and child-surface composition. Do not build this until slice-1 measurements miss the target.

## Next slice

1. Integrate xterm.js WebGL renderer as the slice-1 escape hatch.
2. Build minimal terminal chrome: tabs/status without IDE features.
