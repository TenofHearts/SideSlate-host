#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::mem::size_of;
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};

const DEFAULT_HDC: &str = r"D:\Program\Huawei\DevEco Studio\sdk\default\openharmony\toolchains\hdc.exe";
const DEFAULT_FFMPEG: &str = r"D:\Program\ffmpeg-8.1.1\bin\ffmpeg.exe";
const MAGIC: &[u8; 4] = b"T2H5";
const FLAG_KEY_CONFIG: u32 = 1;
const FLAG_VCL: u32 = 2;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Serialize)]
struct DisplayInfo {
    id: usize,
    name: String,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    primary: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamConfig {
    hdc_path: String,
    ffmpeg_path: String,
    encoder: String,
    display_id: usize,
    fps: u32,
    bitrate: String,
    bufsize: String,
    gop: u32,
    scale: String,
    host: String,
    port: u16,
}

#[derive(Clone, Serialize)]
struct StreamStats {
    running: bool,
    status: String,
    encoder: String,
    packets: u64,
    vcl_packets: u64,
    bytes: u64,
    fps: f64,
    mbps: f64,
    elapsed_seconds: f64,
    last_error: String,
}

impl Default for StreamStats {
    fn default() -> Self {
        Self {
            running: false,
            status: "idle".to_string(),
            encoder: "auto".to_string(),
            packets: 0,
            vcl_packets: 0,
            bytes: 0,
            fps: 0.0,
            mbps: 0.0,
            elapsed_seconds: 0.0,
            last_error: String::new(),
        }
    }
}

struct StreamRuntime {
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    handle: Option<JoinHandle<()>>,
}

struct AppStateInner {
    stats: StreamStats,
    runtime: Option<StreamRuntime>,
}

struct AppState(Arc<Mutex<AppStateInner>>);

#[derive(Serialize)]
struct Defaults {
    hdc_path: String,
    ffmpeg_path: String,
}

#[tauri::command]
fn get_defaults() -> Defaults {
    Defaults {
        hdc_path: DEFAULT_HDC.to_string(),
        ffmpeg_path: DEFAULT_FFMPEG.to_string(),
    }
}

#[tauri::command]
fn list_displays() -> Result<Vec<DisplayInfo>, String> {
    enumerate_displays()
}

#[tauri::command]
fn list_hdc_targets(hdc_path: String) -> Result<String, String> {
    run_text_command(&hdc_path, &["list", "targets"])
}

#[tauri::command]
fn setup_hdc_forward(hdc_path: String) -> Result<String, String> {
    let output = reset_hdc_forward(&hdc_path)?;
    let list = run_text_command(&hdc_path, &["fport", "ls"])?;
    Ok(format!("{}\n{}", output.trim(), list.trim()))
}

#[tauri::command]
fn get_stream_stats(state: State<AppState>) -> Result<StreamStats, String> {
    let guard = state.0.lock().map_err(|_| "state lock poisoned".to_string())?;
    Ok(guard.stats.clone())
}

#[tauri::command]
fn start_stream(app: AppHandle, state: State<AppState>, config: StreamConfig) -> Result<(), String> {
    stop_existing_stream(&state)?;

    let displays = enumerate_displays()?;
    let display = displays
        .into_iter()
        .find(|display| display.id == config.display_id)
        .ok_or_else(|| format!("display {} not found", config.display_id))?;

    let encoder = choose_encoder(&config.ffmpeg_path, &config.encoder)?;
    let command = build_ffmpeg_command(&config, &display, &encoder);
    let stop = Arc::new(AtomicBool::new(false));
    let child_slot = Arc::new(Mutex::new(None));

    {
        let mut guard = state.0.lock().map_err(|_| "state lock poisoned".to_string())?;
        guard.stats = StreamStats {
            running: true,
            status: format!("starting {}", encoder),
            encoder: encoder.clone(),
            ..StreamStats::default()
        };
    }

    let state_for_thread = state.0.clone();
    let stop_for_thread = stop.clone();
    let child_for_thread = child_slot.clone();
    let handle = thread::spawn(move || {
        stream_thread(app, state_for_thread, stop_for_thread, child_for_thread, config, command, encoder);
    });

    let mut guard = state.0.lock().map_err(|_| "state lock poisoned".to_string())?;
    guard.runtime = Some(StreamRuntime {
        stop,
        child: child_slot,
        handle: Some(handle),
    });
    Ok(())
}

#[tauri::command]
fn stop_stream(state: State<AppState>, hdc_path: Option<String>) -> Result<(), String> {
    stop_existing_stream(&state)?;
    if let Some(path) = hdc_path {
        if !path.trim().is_empty() {
            close_hdc_forward(&path)?;
        }
    }
    Ok(())
}

fn stop_existing_stream(state: &State<AppState>) -> Result<(), String> {
    let runtime = {
        let mut guard = state.0.lock().map_err(|_| "state lock poisoned".to_string())?;
        guard.runtime.take()
    };

    if let Some(mut runtime) = runtime {
        runtime.stop.store(true, Ordering::SeqCst);
        if let Ok(mut child_guard) = runtime.child.lock() {
            if let Some(child) = child_guard.as_mut() {
                let _ = child.kill();
            }
        }
        if let Some(handle) = runtime.handle.take() {
            let _ = handle.join();
        }
    }

    let mut guard = state.0.lock().map_err(|_| "state lock poisoned".to_string())?;
    guard.stats.running = false;
    guard.stats.status = "stopped".to_string();
    Ok(())
}

fn run_text_command(program: &str, args: &[&str]) -> Result<String, String> {
    let mut command = hidden_command(program);
    let output = command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| format!("failed to run {}: {}", program, err))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!("{} failed: {}{}", program, stdout, stderr));
    }
    Ok(format!("{}{}", stdout, stderr))
}

fn enumerate_displays() -> Result<Vec<DisplayInfo>, String> {
    unsafe extern "system" fn callback(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let displays = &mut *(data.0 as *mut Vec<DisplayInfo>);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut _).as_bool() {
            let rect = info.monitorInfo.rcMonitor;
            let end = info
                .szDevice
                .iter()
                .position(|ch| *ch == 0)
                .unwrap_or(info.szDevice.len());
            let name = String::from_utf16_lossy(&info.szDevice[..end]);
            displays.push(DisplayInfo {
                id: displays.len(),
                name,
                left: rect.left,
                top: rect.top,
                width: rect.right - rect.left,
                height: rect.bottom - rect.top,
                primary: info.monitorInfo.dwFlags & 1 == 1,
            });
        }
        BOOL(1)
    }

    let mut displays = Vec::new();
    let ok = unsafe {
        EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(callback),
            LPARAM(&mut displays as *mut Vec<DisplayInfo> as isize),
        )
    };
    if !ok.as_bool() {
        return Err("EnumDisplayMonitors failed".to_string());
    }
    Ok(displays)
}

fn available_encoders(ffmpeg: &str) -> Result<String, String> {
    run_text_command(ffmpeg, &["-hide_banner", "-encoders"])
}

fn encoder_works(ffmpeg: &str, encoder: &str) -> bool {
    let mut command = hidden_command(ffmpeg);
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=640x360:rate=30:duration=0.2",
        "-frames:v",
        "1",
        "-c:v",
        encoder,
    ]);
    if encoder == "libx265" {
        command.args(["-preset", "ultrafast"]);
    }
    command.args(["-f", "null", "-"]);

    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn choose_encoder(ffmpeg: &str, requested: &str) -> Result<String, String> {
    if requested != "auto" {
        if encoder_works(ffmpeg, requested) {
            return Ok(requested.to_string());
        }
        return Err(format!("requested encoder failed runtime check: {}", requested));
    }

    let encoders = available_encoders(ffmpeg)?;
    for encoder in ["hevc_nvenc", "hevc_qsv", "libx265"] {
        if encoders.contains(encoder) && encoder_works(ffmpeg, encoder) {
            return Ok(encoder.to_string());
        }
    }
    Err("no working HEVC encoder found".to_string())
}

fn parse_size_to_kbits(size: &str) -> u32 {
    let upper = size.trim().to_ascii_uppercase();
    let digits = upper.trim_end_matches(['K', 'M']);
    let value = digits.parse::<u32>().unwrap_or(1).max(1);
    if upper.ends_with('M') {
        value * 1000
    } else {
        value
    }
}

fn build_ffmpeg_command(config: &StreamConfig, display: &DisplayInfo, encoder: &str) -> Vec<String> {
    let mut command = vec![
        config.ffmpeg_path.clone(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "warning".into(),
        "-probesize".into(),
        "32".into(),
        "-analyzeduration".into(),
        "0".into(),
        "-fflags".into(),
        "nobuffer".into(),
        "-flags".into(),
        "low_delay".into(),
        "-f".into(),
        "gdigrab".into(),
        "-draw_mouse".into(),
        "1".into(),
        "-framerate".into(),
        config.fps.to_string(),
        "-offset_x".into(),
        display.left.to_string(),
        "-offset_y".into(),
        display.top.to_string(),
        "-video_size".into(),
        format!("{}x{}", display.width, display.height),
        "-i".into(),
        "desktop".into(),
    ];

    if !config.scale.trim().is_empty() {
        command.extend(["-vf".into(), format!("scale={}", config.scale.trim())]);
    }

    match encoder {
        "hevc_nvenc" => command.extend([
            "-c:v".into(),
            "hevc_nvenc".into(),
            "-preset".into(),
            "p1".into(),
            "-tune".into(),
            "ull".into(),
            "-rc".into(),
            "cbr".into(),
            "-b:v".into(),
            config.bitrate.clone(),
            "-maxrate".into(),
            config.bitrate.clone(),
            "-bufsize".into(),
            config.bufsize.clone(),
            "-g".into(),
            config.gop.to_string(),
            "-bf".into(),
            "0".into(),
        ]),
        "hevc_qsv" => command.extend([
            "-c:v".into(),
            "hevc_qsv".into(),
            "-preset".into(),
            "veryfast".into(),
            "-b:v".into(),
            config.bitrate.clone(),
            "-maxrate".into(),
            config.bitrate.clone(),
            "-bufsize".into(),
            config.bufsize.clone(),
            "-g".into(),
            config.gop.to_string(),
            "-bf".into(),
            "0".into(),
            "-look_ahead".into(),
            "0".into(),
        ]),
        _ => command.extend([
            "-c:v".into(),
            "libx265".into(),
            "-preset".into(),
            "ultrafast".into(),
            "-tune".into(),
            "zerolatency".into(),
            "-b:v".into(),
            config.bitrate.clone(),
            "-bufsize".into(),
            config.bufsize.clone(),
            "-x265-params".into(),
            format!(
                "keyint={}:min-keyint={}:scenecut=0:bframes=0:vbv-bufsize={}",
                config.gop,
                config.gop,
                parse_size_to_kbits(&config.bufsize)
            ),
        ]),
    }

    command.extend([
        "-an".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-f".into(),
        "hevc".into(),
        "pipe:1".into(),
    ]);
    command
}

fn stream_thread(
    app: AppHandle,
    state: Arc<Mutex<AppStateInner>>,
    stop: Arc<AtomicBool>,
    child_slot: Arc<Mutex<Option<Child>>>,
    config: StreamConfig,
    command: Vec<String>,
    encoder: String,
) {
    let result = run_stream_loop(&app, &state, &stop, &child_slot, &config, &command, &encoder);
    let mut guard = match state.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    guard.stats.running = false;
    if let Err(error) = result {
        guard.stats.status = "error".to_string();
        guard.stats.last_error = error;
    } else if guard.stats.last_error.is_empty() {
        guard.stats.status = "stopped".to_string();
    }
    let _ = app.emit("stream-stats", guard.stats.clone());
}

fn run_stream_loop(
    app: &AppHandle,
    state: &Arc<Mutex<AppStateInner>>,
    stop: &Arc<AtomicBool>,
    child_slot: &Arc<Mutex<Option<Child>>>,
    config: &StreamConfig,
    command: &[String],
    encoder: &str,
) -> Result<(), String> {
    if !config.hdc_path.trim().is_empty() {
        update_status(state, app, true, "resetting HDC forward", encoder, "");
        reset_hdc_forward(&config.hdc_path)
            .map_err(|err| format!("HDC forward reset failed: {}", err))?;
    }

    update_status(state, app, true, &format!("connecting to {}:{}", config.host, config.port), encoder, "");
    let mut stream = connect_with_retry(config, state, app, encoder)?;
    stream
        .set_nodelay(true)
        .map_err(|err| format!("set TCP_NODELAY failed: {}", err))?;

    update_status(state, app, true, "starting ffmpeg", encoder, "");
    let mut child = hidden_command(&command[0])
        .args(&command[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("ffmpeg start failed: {}", err))?;
    let mut stdout = child.stdout.take().ok_or_else(|| "ffmpeg stdout unavailable".to_string())?;
    {
        let mut guard = child_slot.lock().map_err(|_| "child lock poisoned".to_string())?;
        *guard = Some(child);
    }

    update_status(state, app, true, "streaming", encoder, "");
    let start = Instant::now();
    let mut last_emit = Instant::now();
    let mut seq = 0u32;
    let mut packets = 0u64;
    let mut vcl_packets = 0u64;
    let mut bytes = 0u64;
    let mut parser = AnnexBParser::default();
    let mut read_buffer = [0u8; 64 * 1024];

    while !stop.load(Ordering::SeqCst) {
        let read = stdout.read(&mut read_buffer).map_err(|err| format!("ffmpeg read failed: {}", err))?;
        if read == 0 {
            break;
        }
        let nals = parser.push(&read_buffer[..read]);
        for nal in nals {
            let timestamp_us = start.elapsed().as_micros() as u64;
            let flags = flags_for_nal(&nal);
            send_packet(&mut stream, seq, timestamp_us, flags, &nal)
                .map_err(|err| format!("{} while sending packet seq={} flags={} bytes={}", err, seq, flags, nal.len()))?;
            seq = seq.wrapping_add(1);
            packets += 1;
            bytes += nal.len() as u64;
            if flags & FLAG_VCL != 0 {
                vcl_packets += 1;
            }
        }

        if last_emit.elapsed() >= Duration::from_millis(500) {
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let mut guard = state.lock().map_err(|_| "state lock poisoned".to_string())?;
            guard.stats.running = true;
            guard.stats.status = "streaming".to_string();
            guard.stats.encoder = encoder.to_string();
            guard.stats.packets = packets;
            guard.stats.vcl_packets = vcl_packets;
            guard.stats.bytes = bytes;
            guard.stats.fps = vcl_packets as f64 / elapsed;
            guard.stats.mbps = bytes as f64 * 8.0 / elapsed / 1_000_000.0;
            guard.stats.elapsed_seconds = elapsed;
            let _ = app.emit("stream-stats", guard.stats.clone());
            last_emit = Instant::now();
        }
    }

    if let Ok(mut guard) = child_slot.lock() {
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
        }
        *guard = None;
    }
    Ok(())
}

fn hidden_command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn reset_hdc_forward(hdc_path: &str) -> Result<String, String> {
    let mut log = String::new();

    match close_hdc_forward(hdc_path) {
        Ok(output) => log.push_str(&output),
        Err(error) => log.push_str(&format!("HDC forward remove warning: {}\n", error)),
    }

    let targets = run_text_command(hdc_path, &["list", "targets"])?;
    if targets.trim().is_empty() || targets.contains("[Empty]") {
        match run_text_command(hdc_path, &["kill", "-r"]) {
            Ok(output) => log.push_str(&output),
            Err(error) => log.push_str(&format!("HDC restart warning: {}\n", error)),
        }
        thread::sleep(Duration::from_millis(1800));
        let targets_after_restart = run_text_command(hdc_path, &["list", "targets"])?;
        if targets_after_restart.trim().is_empty() || targets_after_restart.contains("[Empty]") {
            return Err("no HDC target after forward reset".to_string());
        }
        log.push_str(&targets_after_restart);
    } else {
        log.push_str(&targets);
    }

    let output = run_text_command(hdc_path, &["fport", "tcp:17005", "tcp:7005"])?;
    log.push_str(&output);
    Ok(log)
}

fn close_hdc_forward(hdc_path: &str) -> Result<String, String> {
    run_text_command(hdc_path, &["fport", "rm", "tcp:17005", "tcp:7005"])
}

fn connect_with_retry(
    config: &StreamConfig,
    state: &Arc<Mutex<AppStateInner>>,
    app: &AppHandle,
    encoder: &str,
) -> Result<TcpStream, String> {
    let mut last_error = String::new();
    for attempt in 1..=30 {
        match TcpStream::connect((config.host.as_str(), config.port)) {
            Ok(stream) => return Ok(stream),
            Err(err) => {
                last_error = err.to_string();
                update_status(
                    state,
                    app,
                    true,
                    &format!("waiting for receiver on {}:{} ({}/30)", config.host, config.port, attempt),
                    encoder,
                    "",
                );
                thread::sleep(Duration::from_millis(300));
            }
        }
    }
    Err(format!(
        "connect failed after HDC setup. Start the tablet receiver, then retry. Last error: {}",
        last_error
    ))
}

fn update_status(state: &Arc<Mutex<AppStateInner>>, app: &AppHandle, running: bool, status: &str, encoder: &str, error: &str) {
    if let Ok(mut guard) = state.lock() {
        guard.stats.running = running;
        guard.stats.status = status.to_string();
        guard.stats.encoder = encoder.to_string();
        guard.stats.last_error = error.to_string();
        let _ = app.emit("stream-stats", guard.stats.clone());
    }
}

fn send_packet(stream: &mut TcpStream, sequence: u32, timestamp_us: u64, flags: u32, payload: &[u8]) -> Result<(), String> {
    let mut packet = Vec::with_capacity(24 + payload.len());
    packet.extend_from_slice(MAGIC);
    packet.extend_from_slice(&sequence.to_le_bytes());
    packet.extend_from_slice(&timestamp_us.to_le_bytes());
    packet.extend_from_slice(&flags.to_le_bytes());
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(payload);
    use std::io::Write;
    stream.write_all(&packet).map_err(|err| format!("socket packet write failed: {}", err))?;
    Ok(())
}

fn flags_for_nal(nal: &[u8]) -> u32 {
    let nal_type = nal_type(nal);
    let mut flags = 0;
    if matches!(nal_type, 32 | 33 | 34) {
        flags |= FLAG_KEY_CONFIG;
    }
    if nal_type <= 31 {
        flags |= FLAG_VCL;
    }
    flags
}

fn nal_type(nal: &[u8]) -> u8 {
    let offset = if nal.starts_with(&[0, 0, 0, 1]) {
        4
    } else if nal.starts_with(&[0, 0, 1]) {
        3
    } else {
        return 255;
    };
    if nal.len() <= offset {
        return 255;
    }
    (nal[offset] >> 1) & 0x3f
}

#[derive(Default)]
struct AnnexBParser {
    buffer: Vec<u8>,
}

impl AnnexBParser {
    fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.buffer.extend_from_slice(data);
        let mut out = Vec::new();
        loop {
            let Some((first, first_len)) = find_start_code(&self.buffer, 0) else {
                self.buffer.clear();
                break;
            };
            let Some((second, _)) = find_start_code(&self.buffer, first + first_len) else {
                if first > 0 {
                    self.buffer.drain(..first);
                }
                break;
            };
            let nal = self.buffer[first..second].to_vec();
            self.buffer.drain(..second);
            if nal.len() > first_len {
                out.push(nal);
            }
        }
        out
    }
}

fn find_start_code(buffer: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut index = start;
    while index + 3 <= buffer.len() {
        if index + 4 <= buffer.len() && buffer[index..index + 4] == [0, 0, 0, 1] {
            return Some((index, 4));
        }
        if buffer[index..index + 3] == [0, 0, 1] {
            return Some((index, 3));
        }
        index += 1;
    }
    None
}

fn main() {
    tauri::Builder::default()
        .manage(AppState(Arc::new(Mutex::new(AppStateInner {
            stats: StreamStats::default(),
            runtime: None,
        }))))
        .invoke_handler(tauri::generate_handler![
            get_defaults,
            list_displays,
            list_hdc_targets,
            setup_hdc_forward,
            start_stream,
            stop_stream,
            get_stream_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tablet2Screen host");
}
