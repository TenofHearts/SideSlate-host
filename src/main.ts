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
  virtual_display: boolean;
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
  receiver_running: boolean;
  receiver_decoder_started: boolean;
  receiver_surface_ready: boolean;
  receiver_packets: number;
  receiver_bytes: number;
  receiver_queued_inputs: number;
  receiver_rendered_outputs: number;
  receiver_dropped_packets: number;
  receiver_sequence_gaps: number;
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
              <option value="hevc_qsv">HEVC Intel Quick Sync / Arc</option>
              <option value="libx265">libx265 fallback</option>
            </select>
          </label>
          <label>Capture
            <select id="captureSelect">
              <option value="ddagrab">DXGI zero-copy when possible</option>
              <option value="ddagrab_zero_copy">DXGI zero-copy / NVENC</option>
              <option value="ddagrab_compat">DXGI compatibility</option>
              <option value="gfxcapture">Windows Graphics Capture experimental</option>
              <option value="gdigrab">GDI fallback</option>
            </select>
          </label>
          <label>FPS<input id="fpsInput" type="number" min="1" max="120" value="60" /></label>
          <label>Bitrate<input id="bitrateInput" value="35M" /></label>
          <label>VBV buffer<input id="bufsizeInput" value="2M" /></label>
          <label>GOP<input id="gopInput" type="number" min="1" value="15" /></label>
          <label>Scale<input id="scaleInput" value="2800:1840" /></label>
          <label class="checkboxLabel"><input id="sendPacingInput" type="checkbox" checked /> Pace oversized frame sends</label>
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
        <div class="summary" id="diagnosticSummary">No stream diagnostics yet.</div>
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
const captureSelect = getSelect('captureSelect');
const fpsInput = getInput('fpsInput');
const bitrateInput = getInput('bitrateInput');
const bufsizeInput = getInput('bufsizeInput');
const gopInput = getInput('gopInput');
const scaleInput = getInput('scaleInput');
const sendPacingInput = getInput('sendPacingInput');
const targetInput = getInput('targetInput');
const statusPill = getElement('statusPill');
const logElement = getElement('log');
const errorBox = getElement('errorBox');
const diagnosticSummary = getElement('diagnosticSummary');

let displays: DisplayInfo[] = [];
let lastHostLogAt = 0;

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
      captureBackend: captureSelect.value,
      displayId: Number(displaySelect.value),
      fps: Number(fpsInput.value),
      bitrate: bitrateInput.value,
      bufsize: bufsizeInput.value,
      gop: Number(gopInput.value),
      scale: scaleInput.value,
      sendPacing: sendPacingInput.checked,
      host: host || '127.0.0.1',
      port: Number(portText || 17005),
    };
    await invoke('start_stream', { config });
    const stats = await invoke<StreamStats>('get_stream_stats');
    if (stats.ffmpeg_command) {
      appendLog(`FFmpeg: ${stats.ffmpeg_command}`);
    }
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
    ? `${display.width} x ${display.height}, origin ${display.left},${display.top}${display.primary ? ', primary' : ''}, DXGI ${display.dxgi_adapter_idx ?? '?'}/${display.dxgi_output_idx ?? '?'}${display.virtual_display ? ', virtual' : ''}`
    : 'No display selected.';
}

function renderStats(stats: StreamStats): void {
  getElement('fpsValue').textContent = stats.fps.toFixed(1);
  getElement('mbpsValue').textContent = stats.mbps.toFixed(2);
  getElement('packetValue').textContent = String(stats.packets);
  getElement('elapsedValue').textContent = stats.elapsed_seconds.toFixed(1);
  statusPill.textContent = `${stats.running ? 'Streaming' : 'Idle'} · ${stats.status} · ${stats.encoder}`;
  statusPill.className = `pill ${stats.running ? 'connected' : 'idle'}`;
  diagnosticSummary.textContent =
    `${stats.bottleneck || 'No bottleneck sample yet'} · ` +
    `Host ${stats.current_fps.toFixed(1)} VCL/s, ${stats.current_mbps.toFixed(1)} Mbps · ` +
    `Tablet RX ${stats.receiver_receive_mbps.toFixed(1)} Mbps, input ${stats.receiver_input_fps.toFixed(1)}/s, render ${stats.receiver_render_fps.toFixed(1)}/s · ` +
    `capture ${stats.effective_capture_backend || '-'} · ` +
    `max frame ${(stats.current_max_packet_bytes / 1024).toFixed(0)} KiB · ` +
    `host read gap ${stats.max_read_gap_ms.toFixed(1)} ms, tablet rx/input/render gaps ${stats.receiver_max_receive_gap_ms.toFixed(1)}/${stats.receiver_max_input_gap_ms.toFixed(1)}/${stats.receiver_max_render_gap_ms.toFixed(1)} ms · ` +
    `socket stalls ${stats.socket_write_blocked_events} / ${stats.socket_write_blocked_ms.toFixed(1)} ms, max ${stats.max_socket_write_ms.toFixed(1)} ms · ` +
    `pacing ${stats.paced_packets} / ${stats.paced_sleep_ms.toFixed(1)} ms · ` +
    `queues host/tablet ${stats.sender_queue_depth}/${stats.receiver_queue_depth}, drops host/tablet ${stats.host_dropped_packets}/${stats.receiver_dropped_packets}, resync ${stats.resync_events}/${stats.resync_dropped_access_units} · ` +
    `parser buffered ${stats.parser_buffer_bytes} B`;
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
      `bottleneck="${stats.bottleneck}", ` +
      `tablet=rx ${stats.receiver_receive_mbps.toFixed(1)}Mbps input ${stats.receiver_input_fps.toFixed(1)}/s render ${stats.receiver_render_fps.toFixed(1)}/s drop ${stats.receiver_drop_fps.toFixed(1)}/s, ` +
      `avg=${stats.fps.toFixed(1)} VCL/s ${stats.mbps.toFixed(1)} Mbps, ` +
      `keyframes=${stats.keyframe_packets}, maxFrame=${(stats.max_packet_bytes / 1024).toFixed(0)}KiB, ` +
      `maxKey=${(stats.max_keyframe_bytes / 1024).toFixed(0)}KiB, maxDelta=${(stats.max_delta_frame_bytes / 1024).toFixed(0)}KiB, ` +
      `ffmpegReads=${stats.ffmpeg_reads}, readGapMax=${stats.max_read_gap_ms.toFixed(1)}ms, tabletGaps=${stats.receiver_max_receive_gap_ms.toFixed(1)}/${stats.receiver_max_input_gap_ms.toFixed(1)}/${stats.receiver_max_render_gap_ms.toFixed(1)}ms, ` +
      `socketStalls=${stats.socket_write_blocked_events}/${stats.socket_write_blocked_ms.toFixed(1)}ms max=${stats.max_socket_write_ms.toFixed(1)}ms, ` +
      `paced=${stats.paced_packets}/${stats.paced_sleep_ms.toFixed(1)}ms sendMax=${stats.max_packet_send_ms.toFixed(1)}ms, ` +
      `queue=${stats.sender_queue_depth}/${stats.receiver_queue_depth} drops=${stats.host_dropped_packets}/${stats.receiver_dropped_packets} seqGaps=${stats.receiver_sequence_gaps} resync=${stats.resync_events}/${stats.resync_dropped_access_units}, ` +
      `parserBuffered=${stats.parser_buffer_bytes}B`
  );
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
