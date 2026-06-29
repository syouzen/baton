# baton

Spec-driven terminal core prototype.

Implemented first slice from `AGENTS.md`:

- bounded PTY byte ring with explicit backpressure (`ByteRing`)
- live PTY reader coalescing with configurable frame/capacity policy (`PtyReadConfig`)
- Alacritty-backed VT parser boundary and visible grid snapshot (`TerminalParser`)
- multi-session routing model (`SessionId`, `TerminalSession`, `SessionManager`)
- fixed-visible-line scrollback contract with pluggable spill sink (`ScrollbackSpill`)
- throughput benchmark harness for coalesced PTY output (`measure_pty_throughput`)

This is intentionally a core crate first. The Tauri/webview chrome and renderer can sit on top of these hot-path contracts without pushing raw terminal throughput into JS.

## Verify

```bash
cargo test --all -- --nocapture
cargo check
cargo run --example throughput_bench -- 1048576
```

## Branch workflow

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the repository workflow:
feature branches target `develop`, and release/stabilization PRs promote `develop` to protected `main` after the required `Rust core` CI check passes.

## Next slice

1. Add a Tauri v2 shell with xterm.js + WebGL as the escape-hatch renderer.
2. Expose Tauri command APIs for create/write/resize/kill.
3. Add branch protection and full development workflow docs.
