import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { listen } from '@tauri-apps/api/event';
import {
  createSession,
  killSession,
  listSessions,
  resizeSession,
  runBaselineMeasurement,
  snapshotSession,
  type SessionView,
  type Slice1MeasurementReport,
  type TerminalOutputEvent,
  writeSession,
} from './commands';
import './styles.css';

const app = document.querySelector<HTMLDivElement>('#app');

if (!app) {
  throw new Error('baton shell mount point #app was not found');
}

app.innerHTML = `
  <main class="shell" aria-label="baton terminal shell">
    <header class="chrome-bar" data-testid="chrome-bar">
      <div class="traffic-lights" aria-hidden="true">
        <span class="traffic-light traffic-light--close"></span>
        <span class="traffic-light traffic-light--minimize"></span>
        <span class="traffic-light traffic-light--zoom"></span>
      </div>
      <div class="title-stack">
        <strong>baton</strong>
        <span data-testid="active-session-label">no session</span>
      </div>
      <div class="session-status" aria-label="terminal status" data-testid="session-status">local · ready</div>
      <button class="chrome-action" type="button" data-testid="new-session-button">New</button>
      <button class="chrome-action chrome-action--danger" type="button" data-testid="close-session-button">Close</button>
    </header>

    <section class="workspace" aria-label="terminal workspace">
      <aside class="session-rail" aria-label="sessions" data-testid="session-rail"></aside>

      <section class="terminal-pane" data-testid="terminal-pane" aria-label="terminal region">
        <div class="xterm-host" data-testid="xterm-host"></div>
      </section>

      <aside class="measurement-dashboard" data-testid="measurement-dashboard" aria-label="slice-1 measurements">
        <div class="measurement-header">
          <strong>Slice-1 baseline</strong>
          <button class="chrome-action" type="button" data-testid="run-measurement-button">Run</button>
        </div>
        <div class="metric-grid" data-testid="metric-grid">
          <article class="metric-card"><span>Throughput</span><strong data-metric="throughput">—</strong></article>
          <article class="metric-card"><span>Frames</span><strong data-metric="frames">—</strong></article>
          <article class="metric-card"><span>Input RTT</span><strong data-metric="input">—</strong></article>
          <article class="metric-card"><span>Resize</span><strong data-metric="resize">—</strong></article>
          <article class="metric-card"><span>Renderer frame</span><strong data-metric="renderer">—</strong></article>
        </div>
      </aside>
    </section>
  </main>
`;

const encoder = new TextEncoder();
const status = document.querySelector<HTMLDivElement>('[data-testid="session-status"]');
const activeSessionLabel = document.querySelector<HTMLSpanElement>('[data-testid="active-session-label"]');
const sessionRail = document.querySelector<HTMLElement>('[data-testid="session-rail"]');
const terminalHost = document.querySelector<HTMLDivElement>('[data-testid="xterm-host"]');
const newSessionButton = document.querySelector<HTMLButtonElement>('[data-testid="new-session-button"]');
const closeSessionButton = document.querySelector<HTMLButtonElement>('[data-testid="close-session-button"]');
const runMeasurementButton = document.querySelector<HTMLButtonElement>('[data-testid="run-measurement-button"]');
const metricNodes = new Map<string, HTMLElement>(
  Array.from(document.querySelectorAll<HTMLElement>('[data-metric]')).map((node) => [
    node.dataset.metric ?? '',
    node,
  ]),
);

if (!terminalHost) {
  throw new Error('baton xterm host was not found');
}

let activeSession: SessionView | null = null;

const terminal = new Terminal({
  allowProposedApi: true,
  cursorBlink: true,
  convertEol: true,
  fontFamily: 'SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace',
  fontSize: 13,
  theme: {
    background: '#020617',
    foreground: '#d7e1ff',
    cursor: '#60a5fa',
    selectionBackground: '#1d4ed8',
  },
});
const fitAddon = new FitAddon();
terminal.loadAddon(fitAddon);
terminal.open(terminalHost);
fitAddon.fit();

try {
  const webglAddon = new WebglAddon();
  webglAddon.onContextLoss(() => {
    setStatus('webgl lost · canvas fallback');
    webglAddon.dispose();
  });
  terminal.loadAddon(webglAddon);
} catch (error) {
  console.warn('xterm WebGL renderer unavailable; falling back to canvas/DOM renderer', error);
  setStatus('webgl fallback · ready');
}

terminal.onData((data) => {
  if (!activeSession) {
    return;
  }
  void writeSession(activeSession.id, Array.from(encoder.encode(data))).catch((error) => {
    setStatus(`write error · ${String(error)}`);
  });
});

void bootstrap();

async function bootstrap() {
  const unlistenOutput = await listen<TerminalOutputEvent>('terminal-output', (event) => {
    if (!activeSession || event.payload.sessionId !== activeSession.id) {
      return;
    }
    terminal.write(new Uint8Array(event.payload.bytes));
  });

  window.addEventListener('beforeunload', () => {
    void unlistenOutput();
  });

  const resizeObserver = new ResizeObserver(() => {
    fitAddon.fit();
    if (activeSession) {
      void resizeSession(activeSession.id, terminal.rows, terminal.cols).catch((error) => {
        setStatus(`resize error · ${String(error)}`);
      });
    }
  });
  resizeObserver.observe(terminalHost as HTMLDivElement);

  newSessionButton?.addEventListener('click', () => {
    void openSession();
  });

  closeSessionButton?.addEventListener('click', () => {
    void closeActiveSession();
  });

  runMeasurementButton?.addEventListener('click', () => {
    void runMeasurementDashboard();
  });

  await refreshSessions();
  if (!activeSession) {
    await openSession();
  }
}

async function openSession() {
  try {
    const session = await createSession();
    setActiveSession(session);
    terminal.reset();
    terminal.focus();
    await resizeSession(session.id, terminal.rows, terminal.cols);
    await refreshSessions();
  } catch (error) {
    setStatus(`command error · ${String(error)}`);
  }
}

async function closeActiveSession() {
  if (!activeSession) {
    return;
  }

  const closing = activeSession;
  try {
    await killSession(closing.id);
    activeSession = null;
    terminal.reset();
    await refreshSessions();
  } catch (error) {
    setStatus(`close error · ${String(error)}`);
  }
}

async function refreshSessions() {
  const sessions = await listSessions();
  if (sessions.length > 0 && !activeSession) {
    setActiveSession(sessions[0]);
  }
  renderSessionRail(sessions);
  setStatus(`${sessions.length} session${sessions.length === 1 ? '' : 's'} · ready`);
}

function renderSessionRail(sessions: SessionView[]) {
  if (!sessionRail) {
    return;
  }
  sessionRail.innerHTML = sessions
    .map((session) => {
      const active = activeSession?.id === session.id ? ' session-tab--active' : '';
      return `<button class="session-tab${active}" type="button" data-session-id="${session.id}">terminal-${session.id}</button>`;
    })
    .join('');

  sessionRail.querySelectorAll<HTMLButtonElement>('[data-session-id]').forEach((button) => {
    button.addEventListener('click', () => {
      const session = sessions.find((candidate) => candidate.id === Number(button.dataset.sessionId));
      if (session) {
        setActiveSession(session);
        void restoreSessionScreen(session).then(() => {
          terminal.focus();
          return resizeSession(session.id, terminal.rows, terminal.cols);
        });
      }
    });
  });
}

function setActiveSession(session: SessionView) {
  activeSession = session;
  if (activeSessionLabel) {
    activeSessionLabel.textContent = `terminal-${session.id}`;
  }
  setStatus(`terminal-${session.id} · ${session.cols}x${session.rows}`);
}

async function restoreSessionScreen(session: SessionView) {
  try {
    const snapshot = await snapshotSession(session.id);
    if (activeSession?.id !== session.id) {
      return;
    }

    terminal.reset();
    terminal.resize(snapshot.cols, snapshot.rows);
    terminal.write(snapshot.lines.join('\r\n'));
    setStatus(`terminal-${session.id} · restored`);
  } catch (error) {
    setStatus(`restore error · ${String(error)}`);
  }
}

async function runMeasurementDashboard() {
  setStatus('measurement · running baseline');
  setMetric('throughput', 'running…');
  setMetric('renderer', 'measuring…');

  try {
    const [report, rendererFrameMs] = await Promise.all([
      runBaselineMeasurement(),
      measureRendererFrameMs(),
    ]);
    renderMeasurementReport(report, rendererFrameMs);
    setStatus('measurement · complete');
  } catch (error) {
    setStatus(`measurement error · ${String(error)}`);
  }
}

function renderMeasurementReport(report: Slice1MeasurementReport, rendererFrameMs: number) {
  setMetric('throughput', `${report.throughputMibPerSecond.toFixed(2)} MiB/s`);
  setMetric('frames', `${report.framesRead} frames · ${formatBytes(report.bytesRead)}`);
  setMetric('input', `${report.inputRoundTripMs} ms`);
  setMetric('resize', `${report.resizeLatencyMicros} µs`);
  setMetric('renderer', `${rendererFrameMs.toFixed(2)} ms`);
}

function measureRendererFrameMs(sampleCount = 5): Promise<number> {
  return new Promise((resolve) => {
    const deltas: number[] = [];
    let previous = performance.now();

    const tick = (now: number) => {
      deltas.push(now - previous);
      previous = now;
      if (deltas.length >= sampleCount) {
        resolve(deltas.reduce((sum, delta) => sum + delta, 0) / deltas.length);
        return;
      }
      requestAnimationFrame(tick);
    };

    requestAnimationFrame(tick);
  });
}

function setMetric(key: string, value: string) {
  const node = metricNodes.get(key);
  if (node) {
    node.textContent = value;
  }
}

function formatBytes(bytes: number) {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
  }
  return `${Math.round(bytes / 1024)} KiB`;
}

function setStatus(message: string) {
  if (status) {
    status.textContent = message;
  }
}
