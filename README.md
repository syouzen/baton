# baton

Spec-driven terminal core prototype.

Implemented first slice from `AGENTS.md`:

- bounded PTY byte ring with explicit backpressure (`ByteRing`)
- live PTY reader coalescing with configurable frame/capacity policy (`PtyReadConfig`)
- Alacritty-backed VT parser boundary and visible grid snapshot (`TerminalParser`)
- multi-session routing model (`SessionId`, `TerminalSession`, `SessionManager`)
- fixed-visible-line scrollback contract with pluggable spill sink (`ScrollbackSpill`)
- PTY resize control path shared by kernel winsize and parser viewport (`TerminalSize`)

This is intentionally a core crate first. The Tauri/webview chrome and renderer can sit on top of these hot-path contracts without pushing raw terminal throughput into JS.

## Verify

```bash
cargo test --all -- --nocapture
cargo check
```

## Next slice

1. Wire `LocalPty` reader into a parser boundary (`alacritty_terminal` or equivalent adapter).
2. Add a Tauri v2 shell with xterm.js + WebGL as the escape-hatch renderer.
3. Add throughput/idle benchmarks for the spec targets.
