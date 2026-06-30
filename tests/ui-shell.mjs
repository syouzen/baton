import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';

const index = await readFile(new URL('../index.html', import.meta.url), 'utf8');
const app = await readFile(new URL('../src/main.ts', import.meta.url), 'utf8');
const commands = await readFile(new URL('../src/commands.ts', import.meta.url), 'utf8');
const css = await readFile(new URL('../src/styles.css', import.meta.url), 'utf8');
const nativeGpuDoc = await readFile(new URL('../docs/native-gpu-spike.md', import.meta.url), 'utf8');
const tauriConfig = JSON.parse(
  await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
);

assert.match(index, /id="app"/, 'frontend must mount into #app');
assert.match(app, /data-testid="terminal-pane"/, 'shell must reserve a terminal pane');
assert.match(app, /data-testid="chrome-bar"/, 'shell must include minimal chrome bar');
assert.doesNotMatch(app, /SCM|Git Graph|Settings/i, 'issue #8 shell must not add IDE features');
assert.match(css, /\.terminal-pane/, 'terminal pane must have explicit styling');
assert.match(commands, /new Channel<ArrayBuffer>\(\)/, 'frontend must stream PTY output over a binary Tauri Channel');
assert.match(commands, /invoke<SessionView>\('create_session'/, 'frontend must call create_session');
assert.match(commands, /invoke<void>\('write_session'/, 'frontend must call write_session');
assert.match(commands, /invoke<SessionView>\('resize_session'/, 'frontend must call resize_session');
assert.match(commands, /invoke<void>\('kill_session'/, 'frontend must call kill_session');
assert.match(commands, /invoke<SessionView\[]>\('list_sessions'/, 'frontend must call list_sessions');
assert.match(commands, /invoke<ArrayBuffer>\('read_session'/, 'read_session smoke path must return raw bytes, not a JSON number array DTO');
assert.match(app, /@xterm\/xterm/, 'frontend must use xterm.js renderer');
assert.match(app, /@xterm\/addon-webgl/, 'frontend must attempt WebGL addon');
assert.doesNotMatch(app, /listen<.*terminal-output/, 'PTY output must not use JSON event IPC');
assert.match(app, /writeSession\(session\.id/, 'terminal input must write back to PTY');
assert.match(app, /data-testid="close-session-button"/, 'chrome must expose close session control');
assert.match(app, /data-testid="measurement-dashboard"/, 'shell must expose slice-1 measurement dashboard');
assert.match(app, /runBaselineMeasurement/, 'dashboard must trigger baseline measurement command');
assert.match(commands, /invoke<Slice1MeasurementReport>\('run_baseline_measurement'/, 'frontend must call run_baseline_measurement');
assert.match(css, /\.measurement-dashboard/, 'measurement dashboard must have explicit styling');
assert.match(css, /\.metric-card/, 'measurement metrics must render as cards');
assert.match(commands, /invoke<TerminalSnapshotView>\('snapshot_session'/, 'frontend must call snapshot_session');
assert.match(app, /new Map<number, TerminalInstance>/, 'session switching must retain one xterm buffer per session');
assert.doesNotMatch(app, /terminal\.reset\(\)/, 'session switching must not reset and replay plain-text snapshots');
assert.doesNotMatch(app, /snapshot\.lines\.join/, 'session switching must not replay plain text and drop styles');
assert.match(app, /requestAnimationFrame/, 'dashboard must record renderer frame timing in the webview');
assert.match(css, /\.xterm-host/, 'xterm host must have explicit styling');
assert.match(nativeGpuDoc, /Entry criteria/, 'native GPU spike document must define entry criteria');
assert.match(nativeGpuDoc, /go\/no-go/i, 'native GPU spike document must provide a go/no-go decision');
assert.equal(tauriConfig.productName, 'baton');
assert.equal(tauriConfig.app.windows[0].title, 'baton');
assert.ok(tauriConfig.build.devUrl.includes('127.0.0.1'), 'dev server should bind localhost');
