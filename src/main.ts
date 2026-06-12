import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './style.css';

type DisplayInfo = {
  id: number;
  name: string;
  left: number;
  top: number;
  width: number;
  height: number;
  primary: boolean;
};

type StreamStats = {
  running: boolean;
  status: string;
  encoder: string;
  packets: number;
  vcl_packets: number;
  bytes: number;
  fps: number;
  mbps: number;
  elapsed_seconds: number;
  last_error: string;
};

type Defaults = {
  hdc_path: string;
  ffmpeg_path: string;
};

const app = document.querySelector<HTMLDivElement>('#app');

if (!app) {
  throw new Error('missing app root');
}

app.innerHTML = `
  <aside class="sidebar">
    <div class="brand">Tablet2Screen</div>
    <nav>
      <a class="active">Dashboard</a>
      <a>USB/HDC</a>
      <a>Stream</a>
      <a>Diagnostics</a>
    </nav>
  </aside>
  <main class="content">
    <header>
      <div>
        <h1>Live HDC Screen Streaming</h1>
        <p>Phase 6 prototype: Windows capture to HarmonyOS native H.265 renderer.</p>
      </div>
      <div id="statusPill" class="pill idle">Idle</div>
    </header>

    <section class="grid">
      <article class="card connection">
        <h2>Connection</h2>
        <label>HDC executable</label>
        <input id="hdcPath" spellcheck="false" />
        <div class="buttonRow">
          <button id="listTargetsButton" class="secondary">List Targets</button>
          <button id="setupHdcButton">Setup HDC</button>
        </div>
        <p class="hint">Start Streaming resets HDC, then runs <code>hdc fport tcp:17005 tcp:7005</code>. Start the tablet receiver first.</p>
      </article>

      <article class="card display">
        <h2>Display</h2>
        <label>Capture display</label>
        <select id="displaySelect"></select>
        <div id="displaySummary" class="summary">No display loaded.</div>
        <button id="refreshDisplaysButton" class="secondary">Refresh Displays</button>
      </article>

      <article class="card stream">
        <h2>Stream</h2>
        <div class="formGrid">
          <label>FFmpeg executable<input id="ffmpegPath" spellcheck="false" /></label>
          <label>Encoder
            <select id="encoderSelect">
              <option value="auto">Auto</option>
              <option value="hevc_nvenc">HEVC NVENC</option>
              <option value="hevc_qsv">HEVC Quick Sync</option>
              <option value="libx265">libx265 fallback</option>
            </select>
          </label>
          <label>FPS<input id="fpsInput" type="number" min="1" max="120" value="30" /></label>
          <label>Bitrate<input id="bitrateInput" value="20M" /></label>
          <label>VBV buffer<input id="bufsizeInput" value="1M" /></label>
          <label>GOP<input id="gopInput" type="number" min="1" value="15" /></label>
          <label>Scale<input id="scaleInput" value="1920:1080" /></label>
          <label>Forwarded host:port<input id="targetInput" value="127.0.0.1:17005" /></label>
        </div>
        <div class="buttonRow primaryActions">
          <button id="startButton">Start Streaming</button>
          <button id="stopButton" class="danger">Stop</button>
        </div>
      </article>

      <article class="card health">
        <h2>Health</h2>
        <div class="metrics">
          <div><strong id="fpsValue">0.0</strong><span>VCL/s</span></div>
          <div><strong id="mbpsValue">0.00</strong><span>Mbps</span></div>
          <div><strong id="packetValue">0</strong><span>Packets</span></div>
          <div><strong id="elapsedValue">0.0</strong><span>Seconds</span></div>
        </div>
        <div id="errorBox" class="errorBox hidden"></div>
      </article>
    </section>

    <section class="card diagnostics">
      <div class="diagnosticsHeader">
        <h2>Diagnostics</h2>
        <button id="clearLogButton" class="secondary small">Clear</button>
      </div>
      <pre id="log"></pre>
    </section>
  </main>
`;

const hdcPath = getInput('hdcPath');
const ffmpegPath = getInput('ffmpegPath');
const displaySelect = getSelect('displaySelect');
const displaySummary = getElement('displaySummary');
const encoderSelect = getSelect('encoderSelect');
const fpsInput = getInput('fpsInput');
const bitrateInput = getInput('bitrateInput');
const bufsizeInput = getInput('bufsizeInput');
const gopInput = getInput('gopInput');
const scaleInput = getInput('scaleInput');
const targetInput = getInput('targetInput');
const statusPill = getElement('statusPill');
const logElement = getElement('log');
const errorBox = getElement('errorBox');

let displays: DisplayInfo[] = [];

void listen<StreamStats>('stream-stats', (event) => {
  renderStats(event.payload);
});

getButton('listTargetsButton').addEventListener('click', async () => {
  await runAction('List HDC targets', async () => {
    const output = await invoke<string>('list_hdc_targets', { hdcPath: hdcPath.value });
    appendLog(output.trim() || 'No HDC output.');
  });
});

getButton('setupHdcButton').addEventListener('click', async () => {
  await runAction('Setup HDC forward', async () => {
    const output = await invoke<string>('setup_hdc_forward', { hdcPath: hdcPath.value });
    appendLog(output.trim() || 'HDC forward command completed.');
  });
});

getButton('refreshDisplaysButton').addEventListener('click', async () => {
  await loadDisplays();
});

displaySelect.addEventListener('change', renderSelectedDisplay);

getButton('startButton').addEventListener('click', async () => {
  await runAction('Start stream', async () => {
    const [host, portText] = targetInput.value.split(':');
    const config = {
      hdcPath: hdcPath.value,
      ffmpegPath: ffmpegPath.value,
      encoder: encoderSelect.value,
      displayId: Number(displaySelect.value),
      fps: Number(fpsInput.value),
      bitrate: bitrateInput.value,
      bufsize: bufsizeInput.value,
      gop: Number(gopInput.value),
      scale: scaleInput.value,
      host: host || '127.0.0.1',
      port: Number(portText || 17005),
    };
    await invoke('start_stream', { config });
    appendLog('Stream start requested.');
  });
});

getButton('stopButton').addEventListener('click', async () => {
  await runAction('Stop stream', async () => {
    await invoke('stop_stream', { hdcPath: hdcPath.value });
    appendLog('Stream stopped.');
  });
});

getButton('clearLogButton').addEventListener('click', () => {
  logElement.textContent = '';
});

void bootstrap();

async function bootstrap(): Promise<void> {
  const defaults = await invoke<Defaults>('get_defaults');
  hdcPath.value = defaults.hdc_path;
  ffmpegPath.value = defaults.ffmpeg_path;
  await loadDisplays();
  const stats = await invoke<StreamStats>('get_stream_stats');
  renderStats(stats);
}

async function loadDisplays(): Promise<void> {
  await runAction('Refresh displays', async () => {
    displays = await invoke<DisplayInfo[]>('list_displays');
    displaySelect.innerHTML = '';
    for (const display of displays) {
      const option = document.createElement('option');
      option.value = String(display.id);
      option.textContent = `${display.primary ? 'Primary - ' : ''}${display.name} (${display.width}x${display.height} @ ${display.left},${display.top})`;
      displaySelect.append(option);
    }
    renderSelectedDisplay();
    appendLog(`Loaded ${displays.length} display(s).`);
  });
}

function renderSelectedDisplay(): void {
  const display = displays.find((item) => item.id === Number(displaySelect.value));
  displaySummary.textContent = display
    ? `${display.width} x ${display.height}, origin ${display.left},${display.top}${display.primary ? ', primary' : ''}`
    : 'No display selected.';
}

function renderStats(stats: StreamStats): void {
  getElement('fpsValue').textContent = stats.fps.toFixed(1);
  getElement('mbpsValue').textContent = stats.mbps.toFixed(2);
  getElement('packetValue').textContent = String(stats.packets);
  getElement('elapsedValue').textContent = stats.elapsed_seconds.toFixed(1);
  statusPill.textContent = `${stats.running ? 'Streaming' : 'Idle'} · ${stats.status} · ${stats.encoder}`;
  statusPill.className = `pill ${stats.running ? 'connected' : 'idle'}`;
  if (stats.last_error) {
    errorBox.textContent = stats.last_error;
    errorBox.classList.remove('hidden');
  } else {
    errorBox.classList.add('hidden');
  }
}

async function runAction(label: string, action: () => Promise<void>): Promise<void> {
  appendLog(`${label}...`);
  try {
    await action();
  } catch (error) {
    appendLog(`${label} failed: ${String(error)}`);
  }
}

function appendLog(message: string): void {
  const time = new Date().toLocaleTimeString();
  logElement.textContent = `[${time}] ${message}\n${logElement.textContent ?? ''}`;
}

function getElement(id: string): HTMLElement {
  const element = document.getElementById(id);
  if (!element) throw new Error(`missing #${id}`);
  return element;
}

function getInput(id: string): HTMLInputElement {
  return getElement(id) as HTMLInputElement;
}

function getSelect(id: string): HTMLSelectElement {
  return getElement(id) as HTMLSelectElement;
}

function getButton(id: string): HTMLButtonElement {
  return getElement(id) as HTMLButtonElement;
}
