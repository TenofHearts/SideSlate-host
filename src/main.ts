import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import './style.css';

const appIconUrl = new URL('../src-tauri/icons/icon.ico', import.meta.url).href;

type DisplayInfo = {
  id: number;
  name: string;
  device_string: string;
  left: number;
  top: number;
  width: number;
  height: number;
  primary: boolean;
  virtual_display: boolean;
  hmonitor: number;
  dxgi_adapter_idx: number | null;
  dxgi_output_idx: number | null;
  dxgi_adapter_name: string;
};

type StreamStats = {
  running: boolean;
  status: string;
  encoder: string;
  ffmpeg_command: string;
  packets: number;
  vcl_packets: number;
  keyframe_packets: number;
  bytes: number;
  fps: number;
  mbps: number;
  current_fps: number;
  current_mbps: number;
  current_max_packet_bytes: number;
  max_packet_bytes: number;
  max_keyframe_bytes: number;
  max_delta_frame_bytes: number;
  ffmpeg_reads: number;
  ffmpeg_read_bytes: number;
  parser_buffer_bytes: number;
  current_max_read_gap_ms: number;
  max_read_gap_ms: number;
  socket_write_blocked_events: number;
  socket_write_blocked_ms: number;
  max_socket_write_ms: number;
  paced_packets: number;
  paced_sleep_ms: number;
  max_packet_send_ms: number;
  sender_queue_depth: number;
  host_dropped_packets: number;
  resync_events: number;
  resync_dropped_access_units: number;
  bottleneck: string;
  effective_capture_backend: string;
  video_pipeline: string;
  receiver_running: boolean;
  receiver_decoder_started: boolean;
  receiver_surface_ready: boolean;
  receiver_packets: number;
  receiver_bytes: number;
  receiver_queued_inputs: number;
  receiver_rendered_outputs: number;
  receiver_dropped_packets: number;
  receiver_window_dropped_packets: number;
  receiver_sequence_gaps: number;
  receiver_window_sequence_gaps: number;
  receiver_config_packets: number;
  receiver_keyframes: number;
  receiver_last_sequence: number;
  receiver_queue_depth: number;
  receiver_stream_width: number;
  receiver_stream_height: number;
  receiver_stream_fps: number;
  receiver_last_error: number;
  receiver_receive_mbps: number;
  receiver_input_fps: number;
  receiver_render_fps: number;
  receiver_drop_fps: number;
  receiver_max_receive_gap_ms: number;
  receiver_max_input_gap_ms: number;
  receiver_max_render_gap_ms: number;
  receiver_latest_receive_to_input_ms: number;
  receiver_latest_input_to_render_ms: number;
  receiver_latest_receive_to_render_ms: number;
  receiver_max_receive_to_input_ms: number;
  receiver_max_input_to_render_ms: number;
  receiver_max_receive_to_render_ms: number;
  elapsed_seconds: number;
  last_error: string;
};

type Defaults = {
  hdc_path: string;
  ffmpeg_path: string;
};

type StreamConfig = {
  hdcPath: string;
  ffmpegPath: string;
  encoder: string;
  captureBackend: string;
  displayId: number;
  fps: number;
  bitrate: string;
  bufsize: string;
  gop: number;
  sendPacing: boolean;
  host: string;
  port: number;
};

type PersistedHostSettings = {
  config: StreamConfig;
  displayFingerprint: string | null;
};

const app = document.querySelector<HTMLDivElement>('#app');

if (!app) {
  throw new Error('missing app root');
}

app.innerHTML = `
  <div class="ambient" aria-hidden="true"></div>
  <div class="shell">
    <header class="titlebar" data-tauri-drag-region>
      <div class="titleIdentity" data-tauri-drag-region>
        <img class="appIcon" src="${appIconUrl}" alt="" data-tauri-drag-region />
        <div data-tauri-drag-region>
          <strong>Tablet2Screen</strong>
          <span>Host link console</span>
        </div>
      </div>
      <div class="windowControls">
        <button id="minimizeButton" class="windowButton" aria-label="Minimize">-</button>
        <button id="maximizeButton" class="windowButton" aria-label="Maximize or restore">[]</button>
        <button id="closeButton" class="windowButton close" aria-label="Close">x</button>
      </div>
    </header>

    <main class="dashboard">
      <section class="cardGrid">
        <article class="glassCard connectionCard">
          <div class="cardHeader">
            <span class="cardKicker">Connection</span>
            <h2>Tablet link</h2>
          </div>
          <p class="summary">Prepare HDC transport before streaming. The dashboard will advance automatically as the receiver reports stream health.</p>
          <label>HDC executable<input id="hdcPath" spellcheck="false" /></label>
          <div class="buttonRow">
            <button id="setupHdcButton">Setup Link</button>
            <button id="listTargetsButton" class="secondary">Find Devices</button>
          </div>
          <div class="secondaryCard">
            <span>Forward rule</span>
            <code>tcp:17005 -> tcp:7005</code>
          </div>
        </article>

        <article class="glassCard displayCard">
          <div class="cardHeader">
            <span class="cardKicker">Display</span>
            <h2>Capture source</h2>
          </div>
          <label>Capture display<select id="displaySelect"></select></label>
          <div id="displaySummary" class="summary">No display loaded.</div>
          <button id="refreshDisplaysButton" class="secondary">Refresh Displays</button>
        </article>

        <article class="glassCard streamCard">
          <div class="cardHeader split">
            <div>
              <span class="cardKicker">Streaming</span>
              <h2>Video channel</h2>
            </div>
            <div class="buttonRow primaryActions">
              <button id="startButton">Start Stream</button>
              <button id="restartButton" class="secondary">Restart</button>
              <button id="stopButton" class="danger">Stop</button>
            </div>
          </div>
          <div class="formGrid">
            <label>FFmpeg executable<input id="ffmpegPath" spellcheck="false" /></label>
            <label>Encoder
              <select id="encoderSelect">
                <option value="auto">Auto</option>
                <option value="hevc_nvenc">HEVC NVENC</option>
                <option value="hevc_qsv">HEVC Intel Quick Sync / Arc</option>
                <option value="libx265">libx265 fallback</option>
              </select>
            </label>
            <label>Capture
              <select id="captureSelect">
                <option value="ddagrab">DXGI zero-copy when possible</option>
                <option value="native_mf">Native Media Foundation HEVC</option>
                <option value="ddagrab_zero_copy">DXGI zero-copy / NVENC</option>
                <option value="ddagrab_compat">DXGI compatibility</option>
                <option value="gfxcapture">Windows Graphics Capture experimental</option>
                <option value="gdigrab">GDI fallback</option>
              </select>
            </label>
            <label>FPS<input id="fpsInput" type="number" min="1" max="120" value="60" /></label>
            <label>Bitrate<input id="bitrateInput" value="20M" /></label>
            <label>VBV buffer<input id="bufsizeInput" value="256K" /></label>
            <label>GOP<input id="gopInput" type="number" min="1" value="4" /></label>
            <label>Forwarded host:port<input id="targetInput" value="127.0.0.1:17005" /></label>
            <label class="checkboxLabel"><input id="sendPacingInput" type="checkbox" /> Pace oversized frames</label>
          </div>
        </article>

        <article class="glassCard healthCard">
          <div class="cardHeader">
            <span class="cardKicker">Health</span>
            <h2>Signal quality</h2>
          </div>
          <div class="metrics">
            <div><strong id="fpsValue">0.0</strong><span>VCL/s</span></div>
            <div><strong id="mbpsValue">0.00</strong><span>Mbps</span></div>
            <div><strong id="packetValue">0</strong><span>Packets</span></div>
            <div><strong id="elapsedValue">0.0</strong><span>Seconds</span></div>
          </div>
          <div id="errorBox" class="errorBox hidden"></div>
        </article>

        <article id="diagnosticsCard" class="glassCard diagnosticsCard folded">
          <div class="diagnosticsHeader">
            <div>
              <span class="cardKicker">Recovery</span>
              <h2>Diagnostics</h2>
            </div>
            <div class="buttonRow">
              <button id="toggleDiagnosticsButton" class="secondary small">Show</button>
              <button id="clearLogButton" class="secondary small">Clear</button>
            </div>
          </div>
          <pre id="log"></pre>
        </article>
      </section>
    </main>
  </div>
`;

const hdcPath = getInput('hdcPath');
const ffmpegPath = getInput('ffmpegPath');
const displaySelect = getSelect('displaySelect');
const displaySummary = getElement('displaySummary');
const encoderSelect = getSelect('encoderSelect');
const captureSelect = getSelect('captureSelect');
const fpsInput = getInput('fpsInput');
const bitrateInput = getInput('bitrateInput');
const bufsizeInput = getInput('bufsizeInput');
const gopInput = getInput('gopInput');
const sendPacingInput = getInput('sendPacingInput');
const targetInput = getInput('targetInput');
const titlebar = document.querySelector<HTMLElement>('.titlebar');
const logElement = getElement('log');
const errorBox = getElement('errorBox');
const diagnosticsCard = getElement('diagnosticsCard');
const startButton = getButton('startButton');
const restartButton = getButton('restartButton');
const stopButton = getButton('stopButton');
const toggleDiagnosticsButton = getButton('toggleDiagnosticsButton');

const maxLogBytes = 128 * 1024;
const settingsSaveDelayMs = 300;

let displays: DisplayInfo[] = [];
let lastHostLogAt = 0;
let settingsSaveTimer: number | undefined;
let suppressSettingsSave = false;
const currentWindow = getCurrentWindow();

titlebar?.addEventListener('dblclick', (event) => {
  if ((event.target as HTMLElement).closest('button')) return;
  void toggleFullscreen();
});

getButton('minimizeButton').addEventListener('click', () => {
  void currentWindow.minimize();
});

getButton('maximizeButton').addEventListener('click', () => {
  void currentWindow.toggleMaximize();
});

getButton('closeButton').addEventListener('click', () => {
  void currentWindow.close();
});

void listen<StreamStats>('stream-stats', (event) => {
  renderStats(event.payload);
});

void listen<number>('display-selected', (event) => {
  displaySelect.value = String(event.payload);
  syncCustomSelect(displaySelect);
  renderSelectedDisplay();
});

void listen<string>('tray-error', (event) => {
  appendLog(`Tray action failed: ${event.payload}`);
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

displaySelect.addEventListener('change', () => {
  syncCustomSelect(displaySelect);
  renderSelectedDisplay();
  void invoke('select_display', { displayId: Number(displaySelect.value) });
  scheduleSaveSettings();
});

for (const select of [displaySelect, encoderSelect, captureSelect]) {
  enhanceSelect(select);
}

for (const control of [
  hdcPath,
  ffmpegPath,
  fpsInput,
  bitrateInput,
  bufsizeInput,
  gopInput,
  sendPacingInput,
  targetInput,
]) {
  control.addEventListener('input', scheduleSaveSettings);
  control.addEventListener('change', scheduleSaveSettings);
}

for (const select of [encoderSelect, captureSelect]) {
  select.addEventListener('change', () => {
    syncCustomSelect(select);
    scheduleSaveSettings();
  });
}

startButton.addEventListener('click', async () => {
  await startStream('Start stream');
});

restartButton.addEventListener('click', async () => {
  await runAction('Restart stream', async () => {
    await invoke('stop_stream', { hdcPath: hdcPath.value });
    await startStreamRequest();
    appendLog('Stream restart requested.');
  });
});

stopButton.addEventListener('click', async () => {
  await runAction('Stop stream', async () => {
    await invoke('stop_stream', { hdcPath: hdcPath.value });
    appendLog('Stream stopped.');
  });
});

getButton('clearLogButton').addEventListener('click', () => {
  logElement.textContent = '';
});

toggleDiagnosticsButton.addEventListener('click', () => {
  const folded = diagnosticsCard.classList.toggle('folded');
  toggleDiagnosticsButton.textContent = folded ? 'Show' : 'Hide';
});

void bootstrap();

async function bootstrap(): Promise<void> {
  const defaults = await invoke<Defaults>('get_defaults');
  const settings = await invoke<PersistedHostSettings>('get_host_settings');
  suppressSettingsSave = true;
  hdcPath.value = settings.config.hdcPath || defaults.hdc_path;
  ffmpegPath.value = settings.config.ffmpegPath || defaults.ffmpeg_path;
  encoderSelect.value = settings.config.encoder || 'auto';
  captureSelect.value = settings.config.captureBackend || 'ddagrab';
  fpsInput.value = String(settings.config.fps || 60);
  bitrateInput.value = settings.config.bitrate || '20M';
  bufsizeInput.value = settings.config.bufsize || '256K';
  gopInput.value = String(settings.config.gop || 4);
  sendPacingInput.checked = settings.config.sendPacing;
  targetInput.value = `${settings.config.host || '127.0.0.1'}:${settings.config.port || 17005}`;
  suppressSettingsSave = false;
  await loadDisplays();
  applyPersistedDisplay(settings);
  syncCustomSelect(encoderSelect);
  syncCustomSelect(captureSelect);
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
    syncCustomSelect(displaySelect);
    renderSelectedDisplay();
    appendLog(`Loaded ${displays.length} display(s).`);
  });
}

function applyPersistedDisplay(settings: PersistedHostSettings): void {
  const persistedDisplay = findPersistedDisplay(settings);
  const displayId = persistedDisplay?.id ?? settings.config.displayId;
  if (displaySelect.querySelector(`option[value="${displayId}"]`)) {
    displaySelect.value = String(displayId);
  }
  syncCustomSelect(displaySelect);
  renderSelectedDisplay();
}

function findPersistedDisplay(settings: PersistedHostSettings): DisplayInfo | undefined {
  if (!settings.displayFingerprint) return undefined;
  return displays.find((display) => displayFingerprint(display) === settings.displayFingerprint);
}

function displayFingerprint(display: DisplayInfo): string {
  return [
    display.name,
    display.device_string,
    display.left,
    display.top,
    display.width,
    display.height,
    display.primary,
    display.dxgi_adapter_idx ?? '',
    display.dxgi_output_idx ?? '',
  ].join('|');
}

function renderSelectedDisplay(): void {
  const display = displays.find((item) => item.id === Number(displaySelect.value));
  displaySummary.textContent = display
    ? `${display.width} x ${display.height}, origin ${display.left},${display.top}${display.primary ? ', primary' : ''}, DXGI ${display.dxgi_adapter_idx ?? '?'}/${display.dxgi_output_idx ?? '?'}${display.virtual_display ? ', virtual' : ''}`
    : 'No display selected.';
}

function renderStats(stats: StreamStats): void {
  getElement('fpsValue').textContent = stats.fps.toFixed(1);
  getElement('mbpsValue').textContent = stats.mbps.toFixed(2);
  getElement('packetValue').textContent = String(stats.packets);
  getElement('elapsedValue').textContent = stats.elapsed_seconds.toFixed(1);
  const isStreaming = stats.running;
  startButton.disabled = isStreaming;
  restartButton.disabled = !isStreaming;
  stopButton.disabled = !isStreaming;
  maybeLogHostDiagnostics(stats);
  if (stats.last_error) {
    errorBox.textContent = stats.last_error;
    errorBox.classList.remove('hidden');
  } else {
    errorBox.classList.add('hidden');
  }
}

function maybeLogHostDiagnostics(stats: StreamStats): void {
  if (!stats.running) return;
  const now = Date.now();
  if (now - lastHostLogAt < 2500) return;
  lastHostLogAt = now;
  appendLog(
    `Host diag: access_unit, now=${stats.current_fps.toFixed(1)} VCL/s ${stats.current_mbps.toFixed(1)} Mbps, ` +
      `bottleneck="${stats.bottleneck}", pipeline=${stats.video_pipeline || 'unknown'}, ` +
      `tablet=rx ${stats.receiver_receive_mbps.toFixed(1)}Mbps input ${stats.receiver_input_fps.toFixed(1)}/s render ${stats.receiver_render_fps.toFixed(1)}/s drop ${stats.receiver_drop_fps.toFixed(1)}/s, ` +
      `avg=${stats.fps.toFixed(1)} VCL/s ${stats.mbps.toFixed(1)} Mbps, ` +
      `keyframes=${stats.keyframe_packets}, maxFrame=${(stats.max_packet_bytes / 1024).toFixed(0)}KiB, ` +
      `maxKey=${(stats.max_keyframe_bytes / 1024).toFixed(0)}KiB, maxDelta=${(stats.max_delta_frame_bytes / 1024).toFixed(0)}KiB, ` +
      `ffmpegReads=${stats.ffmpeg_reads}, readGapNow=${stats.current_max_read_gap_ms.toFixed(1)}ms readGapMax=${stats.max_read_gap_ms.toFixed(1)}ms, tabletGaps=${stats.receiver_max_receive_gap_ms.toFixed(1)}/${stats.receiver_max_input_gap_ms.toFixed(1)}/${stats.receiver_max_render_gap_ms.toFixed(1)}ms, ` +
      `tabletLatency=latest ${stats.receiver_latest_receive_to_input_ms.toFixed(1)}/${stats.receiver_latest_input_to_render_ms.toFixed(1)}/${stats.receiver_latest_receive_to_render_ms.toFixed(1)}ms max ${stats.receiver_max_receive_to_input_ms.toFixed(1)}/${stats.receiver_max_input_to_render_ms.toFixed(1)}/${stats.receiver_max_receive_to_render_ms.toFixed(1)}ms, ` +
      `socketStalls=${stats.socket_write_blocked_events}/${stats.socket_write_blocked_ms.toFixed(1)}ms max=${stats.max_socket_write_ms.toFixed(1)}ms, ` +
      `paced=${stats.paced_packets}/${stats.paced_sleep_ms.toFixed(1)}ms sendMax=${stats.max_packet_send_ms.toFixed(1)}ms, ` +
      `queue=${stats.sender_queue_depth}/${stats.receiver_queue_depth} drops=${stats.host_dropped_packets}/${stats.receiver_window_dropped_packets}/${stats.receiver_dropped_packets} seqGaps=${stats.receiver_window_sequence_gaps}/${stats.receiver_sequence_gaps} resync=${stats.resync_events}/${stats.resync_dropped_access_units}, ` +
      `parserBuffered=${stats.parser_buffer_bytes}B`
  );
}

async function startStream(label: string): Promise<void> {
  await runAction(label, async () => {
    await startStreamRequest();
    appendLog('Stream start requested.');
  });
}

async function startStreamRequest(): Promise<void> {
  await saveSettingsNow();
  const config = buildStreamConfig();
  await invoke('start_stream', { config });
  const stats = await invoke<StreamStats>('get_stream_stats');
  if (stats.ffmpeg_command) {
    appendLog(`FFmpeg: ${stats.ffmpeg_command}`);
  }
}

function buildStreamConfig(): StreamConfig {
  const [host, portText] = targetInput.value.split(':');
  return {
    hdcPath: hdcPath.value,
    ffmpegPath: ffmpegPath.value,
    encoder: encoderSelect.value,
    captureBackend: captureSelect.value,
    displayId: Number(displaySelect.value),
    fps: Number(fpsInput.value),
    bitrate: bitrateInput.value,
    bufsize: bufsizeInput.value,
    gop: Number(gopInput.value),
    sendPacing: sendPacingInput.checked,
    host: host || '127.0.0.1',
    port: Number(portText || 17005),
  };
}

function scheduleSaveSettings(): void {
  if (suppressSettingsSave) return;
  window.clearTimeout(settingsSaveTimer);
  settingsSaveTimer = window.setTimeout(() => {
    void saveSettingsNow();
  }, settingsSaveDelayMs);
}

async function saveSettingsNow(): Promise<void> {
  window.clearTimeout(settingsSaveTimer);
  settingsSaveTimer = undefined;
  const display = displays.find((item) => item.id === Number(displaySelect.value));
  await invoke('save_host_settings', {
    settings: {
      config: buildStreamConfig(),
      displayFingerprint: display ? displayFingerprint(display) : null,
    } satisfies PersistedHostSettings,
  });
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
  const nextLog = `[${time}] ${message}\n${logElement.textContent ?? ''}`;
  logElement.textContent = trimLogToLimit(nextLog);
}

function trimLogToLimit(log: string): string {
  if (new Blob([log]).size <= maxLogBytes) return log;
  let trimmed = log;
  while (new Blob([trimmed]).size > maxLogBytes) {
    const lastLineStart = trimmed.lastIndexOf('\n', trimmed.length - 2);
    if (lastLineStart <= 0) return trimmed.slice(0, maxLogBytes);
    trimmed = trimmed.slice(0, lastLineStart + 1);
  }
  return trimmed;
}

async function toggleFullscreen(): Promise<void> {
  const isFullscreen = await currentWindow.isFullscreen();
  await currentWindow.setFullscreen(!isFullscreen);
}

function enhanceSelect(select: HTMLSelectElement): void {
  select.classList.add('nativeSelect');
  const shell = document.createElement('div');
  shell.className = 'customSelect';
  shell.innerHTML = `
    <button type="button" class="selectButton" aria-haspopup="listbox" aria-expanded="false">
      <span></span><b>v</b>
    </button>
    <div class="selectMenu" role="listbox"></div>
  `;
  select.insertAdjacentElement('afterend', shell);

  const button = shell.querySelector<HTMLButtonElement>('.selectButton');
  if (!button) throw new Error('missing custom select button');

  button.addEventListener('click', () => {
    const isOpen = shell.classList.toggle('open');
    button.setAttribute('aria-expanded', String(isOpen));
  });

  document.addEventListener('click', (event) => {
    if (shell.contains(event.target as Node)) return;
    shell.classList.remove('open');
    button.setAttribute('aria-expanded', 'false');
  });

  syncCustomSelect(select);
}

function syncCustomSelect(select: HTMLSelectElement): void {
  const shell = select.nextElementSibling as HTMLElement | null;
  if (!shell?.classList.contains('customSelect')) return;
  const buttonLabel = shell.querySelector<HTMLSpanElement>('.selectButton span');
  const menu = shell.querySelector<HTMLDivElement>('.selectMenu');
  if (!buttonLabel || !menu) return;

  const selected = select.selectedOptions[0];
  buttonLabel.textContent = selected?.textContent ?? 'No choices available';
  menu.innerHTML = '';
  for (const option of Array.from(select.options)) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = option.value === select.value ? 'selectOption selected' : 'selectOption';
    item.textContent = option.textContent;
    item.addEventListener('click', () => {
      select.value = option.value;
      select.dispatchEvent(new Event('change', { bubbles: true }));
      shell.classList.remove('open');
      shell.querySelector('.selectButton')?.setAttribute('aria-expanded', 'false');
    });
    menu.append(item);
  }
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
