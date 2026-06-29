# Native GPU renderer spike

This is a slice-2 plan, not a slice-1 implementation ticket. Baton should keep xterm.js/WebGL as the production terminal renderer until measurements prove the webview path cannot meet the target.

## Entry criteria

Do not start native GPU production work until all of these are true:

1. The slice-1 Tauri + xterm.js/WebGL terminal is merged and usable.
2. PTY output is coalesced before IPC and typing input reaches the PTY reliably.
3. Measurements are recorded for:
   - sustained PTY throughput,
   - renderer frame time while printing large output,
   - idle CPU while a shell is open,
   - resize latency,
   - macOS-specific WebView/WebGL stability.
4. The measurements show slice 1 misses the target or has unacceptable platform behavior.

## Current go/no-go decision

**Go/no-go: NO-GO for production native GPU work right now.**

Reason: slice 1 now has the intended escape-hatch renderer path. Building a native `wgpu` surface before measuring it would add the highest-risk part of the architecture without proof that it is needed.

## Spike scope if entry criteria are met

Keep the spike isolated from the production Tauri/xterm.js path.

1. Research macOS Tauri webview composition:
   - child `NSView`/layer embedding,
   - focus and IME behavior,
   - z-order and clipping with webview chrome.
2. Prototype outside the main app under `experiments/wgpu-terminal` or a dedicated crate.
3. Render a fixed terminal grid snapshot with:
   - glyph atlas,
   - damage-region redraw,
   - cursor blink,
   - basic color attributes.
4. Benchmark against slice-1 xterm.js/WebGL results before any integration decision.
5. Document platform risks and decide whether to continue, postpone, or abandon.

## Production integration guardrails

- Do not replace xterm.js until the native renderer wins on measured throughput and reliability.
- Do not put native GPU code on the command/control path.
- Do not break Tauri webview chrome or PTY command APIs.
- Keep the Rust-owned parser/grid as the renderer boundary.

## Validation for a future spike

A future spike PR should include:

```bash
cargo test --all -- --nocapture
cargo check
npm test
npm run build
```

Plus a benchmark note comparing xterm.js/WebGL and the isolated native prototype on the same recorded workload.
