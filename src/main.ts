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
        <span>slice-1 shell</span>
      </div>
      <div class="session-status" aria-label="terminal status">local · ready</div>
    </header>

    <section class="workspace" aria-label="terminal workspace">
      <aside class="session-rail" aria-label="sessions">
        <button class="session-tab session-tab--active" type="button">terminal-1</button>
      </aside>

      <section class="terminal-pane" data-testid="terminal-pane" aria-label="reserved terminal region">
        <div class="terminal-placeholder">
          <span class="prompt">$</span>
          <span>Terminal renderer mounts here</span>
        </div>
      </section>
    </section>
  </main>
`;
