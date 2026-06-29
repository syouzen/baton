import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';

const index = await readFile(new URL('../index.html', import.meta.url), 'utf8');
const app = await readFile(new URL('../src/main.ts', import.meta.url), 'utf8');
const css = await readFile(new URL('../src/styles.css', import.meta.url), 'utf8');
const tauriConfig = JSON.parse(
  await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
);

assert.match(index, /id="app"/, 'frontend must mount into #app');
assert.match(app, /data-testid="terminal-pane"/, 'shell must reserve a terminal pane');
assert.match(app, /data-testid="chrome-bar"/, 'shell must include minimal chrome bar');
assert.doesNotMatch(app, /SCM|Git Graph|Settings/i, 'issue #8 shell must not add IDE features');
assert.match(css, /\.terminal-pane/, 'terminal pane must have explicit styling');
assert.equal(tauriConfig.productName, 'baton');
assert.equal(tauriConfig.app.windows[0].title, 'baton');
assert.ok(tauriConfig.build.devUrl.includes('127.0.0.1'), 'dev server should bind localhost');
