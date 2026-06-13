#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::mem::size_of;
use std::net::TcpStream;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};

mod protocol;

const DEFAULT_HDC: &str =
    r"D:\Program\Huawei\DevEco Studio\sdk\default\openharmony\toolchains\hdc.exe";
const DEFAULT_FFMPEG: &str = r"D:\Program\ffmpeg-8.1.1\bin\ffmpeg.exe";
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
    capture_backend: String,
    display_id: usize,
    fps: u32,
    bitrate: String,
    bufsize: String,
    gop: u32,
    scale: String,
    send_pacing: bool,
    host: String,
    port: u16,
}

#[derive(Clone, Serialize)]
struct StreamStats {
    running: bool,
    status: String,
    encoder: String,
    ffmpeg_command: String,
    packets: u64,
    vcl_packets: u64,
    keyframe_packets: u64,
    bytes: u64,
    fps: f64,
    mbps: f64,
    current_fps: f64,
    current_mbps: f64,
    current_max_packet_bytes: u64,
    max_packet_bytes: u64,
    max_keyframe_bytes: u64,
    max_delta_frame_bytes: u64,
    ffmpeg_reads: u64,
    ffmpeg_read_bytes: u64,
    parser_buffer_bytes: u64,
    max_read_gap_ms: f64,
    socket_write_blocked_events: u64,
    socket_write_blocked_ms: f64,
    max_socket_write_ms: f64,
    paced_packets: u64,
    paced_sleep_ms: f64,
    max_packet_send_ms: f64,
    sender_queue_depth: u64,
    host_dropped_packets: u64,
    elapsed_seconds: f64,
    last_error: String,
}

impl Default for StreamStats {
    fn default() -> Self {
        Self {
            running: false,
            status: "idle".to_string(),
            encoder: "auto".to_string(),
            ffmpeg_command: String::new(),
            packets: 0,
            vcl_packets: 0,
            keyframe_packets: 0,
            bytes: 0,
            fps: 0.0,
            mbps: 0.0,
            current_fps: 0.0,
            current_mbps: 0.0,
            current_max_packet_bytes: 0,
            max_packet_bytes: 0,
            max_keyframe_bytes: 0,
            max_delta_frame_bytes: 0,
            ffmpeg_reads: 0,
            ffmpeg_read_bytes: 0,
            parser_buffer_bytes: 0,
            max_read_gap_ms: 0.0,
            socket_write_blocked_events: 0,
            socket_write_blocked_ms: 0.0,
            max_socket_write_ms: 0.0,
            paced_packets: 0,
            paced_sleep_ms: 0.0,
            max_packet_send_ms: 0.0,
            sender_queue_depth: 0,
            host_dropped_packets: 0,
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
    let guard = state
        .0
        .lock()
        .map_err(|_| "state lock poisoned".to_string())?;
    Ok(guard.stats.clone())
}

#[tauri::command]
fn start_stream(
    app: AppHandle,
    state: State<AppState>,
    config: StreamConfig,
) -> Result<(), String> {
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
        let mut guard = state
            .0
            .lock()
            .map_err(|_| "state lock poisoned".to_string())?;
        guard.stats = StreamStats {
            running: true,
            status: format!("starting {}", encoder),
            encoder: encoder.clone(),
            ffmpeg_command: quote_command(&command),
            ..StreamStats::default()
        };
    }

    let state_for_thread = state.0.clone();
    let stop_for_thread = stop.clone();
    let child_for_thread = child_slot.clone();
    let handle = thread::spawn(move || {
        stream_thread(
            app,
            state_for_thread,
            stop_for_thread,
            child_for_thread,
            config,
            command,
            encoder,
        );
    });

    let mut guard = state
        .0
        .lock()
        .map_err(|_| "state lock poisoned".to_string())?;
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
        let mut guard = state
            .0
            .lock()
            .map_err(|_| "state lock poisoned".to_string())?;
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

    let mut guard = state
        .0
        .lock()
        .map_err(|_| "state lock poisoned".to_string())?;
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
    unsafe extern "system" fn callback(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
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
        return Err(format!(
            "requested encoder failed runtime check: {}",
            requested
        ));
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

fn quote_command(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part.contains(char::is_whitespace) {
                format!("\"{}\"", part.replace('"', "\\\""))
            } else {
                part.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_dxgi_capture(capture_backend: &str) -> bool {
    matches!(
        capture_backend,
        "ddagrab" | "ddagrab_zero_copy" | "ddagrab_compat"
    )
}

fn use_gpu_resident_dxgi(config: &StreamConfig, encoder: &str) -> bool {
    if encoder != "hevc_nvenc" {
        return false;
    }
    match config.capture_backend.as_str() {
        "ddagrab_zero_copy" => true,
        "ddagrab" => encoder == "hevc_nvenc",
        _ => false,
    }
}

fn parse_scale(scale: &str) -> Option<(u32, u32)> {
    let scale = scale.trim();
    let (width, height) = scale.split_once(':')?;
    let width = width.parse::<u32>().ok()?;
    let height = height.parse::<u32>().ok()?;
    (width > 0 && height > 0).then_some((width, height))
}

fn build_ffmpeg_command(
    config: &StreamConfig,
    display: &DisplayInfo,
    encoder: &str,
) -> Vec<String> {
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
    ];

    match config.capture_backend.as_str() {
        "gdigrab" => command.extend([
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
        ]),
        "gfxcapture" => command.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!(
                "gfxcapture=monitor_idx={}:max_framerate={}:capture_cursor=1:display_border=0:width={}:height={}:output_fmt=bgra",
                display.id, config.fps, display.width, display.height
            ),
        ]),
        _ if is_dxgi_capture(&config.capture_backend) => command.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!(
                "ddagrab=output_idx={}:framerate={}:draw_mouse=1:dup_frames=1:video_size={}x{}:output_fmt=bgra:allow_fallback=1",
                display.id, config.fps, display.width, display.height
            ),
        ]),
        _ => command.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!(
                "ddagrab=output_idx={}:framerate={}:draw_mouse=1:dup_frames=1:video_size={}x{}:output_fmt=bgra:allow_fallback=1",
                display.id, config.fps, display.width, display.height
            ),
        ]),
    }

    let mut filters = Vec::new();
    let gpu_resident_dxgi = use_gpu_resident_dxgi(config, encoder);
    if gpu_resident_dxgi {
        if let Some((width, height)) = parse_scale(&config.scale) {
            filters.push(format!(
                "scale_d3d11=width={}:height={}:format=bgra",
                width, height
            ));
        }
    } else if config.capture_backend != "gdigrab" {
        filters.push("hwdownload".to_string());
        filters.push("format=bgra".to_string());
        if !config.scale.trim().is_empty() {
            filters.push(format!("scale={}", config.scale.trim()));
        }
        filters.push("format=yuv420p".to_string());
    } else if !config.scale.trim().is_empty() {
        filters.push(format!("scale={}", config.scale.trim()));
    }
    if !filters.is_empty() {
        command.extend(["-vf".into(), filters.join(",")]);
    }

    match encoder {
        "hevc_nvenc" => command.extend([
            "-c:v".into(),
            "hevc_nvenc".into(),
            "-preset".into(),
            "p1".into(),
            "-tune".into(),
            "ull".into(),
            "-zerolatency".into(),
            "1".into(),
            "-delay".into(),
            "0".into(),
            "-rc-lookahead".into(),
            "0".into(),
            "-surfaces".into(),
            "2".into(),
            "-dpb_size".into(),
            "1".into(),
            "-forced-idr".into(),
            "1".into(),
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
            "-rgb_mode".into(),
            "yuv420".into(),
        ]),
        "hevc_qsv" => command.extend([
            "-c:v".into(),
            "hevc_qsv".into(),
            "-preset".into(),
            "veryfast".into(),
            "-async_depth".into(),
            "1".into(),
            "-low_delay_brc".into(),
            "1".into(),
            "-forced_idr".into(),
            "1".into(),
            "-scenario".into(),
            "remotegaming".into(),
            "-gpb".into(),
            "0".into(),
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
            "-look_ahead_depth".into(),
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
        "-flush_packets".into(),
        "1".into(),
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
    let result = run_stream_loop(
        &app,
        &state,
        &stop,
        &child_slot,
        &config,
        &command,
        &encoder,
    );
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

    update_status(
        state,
        app,
        true,
        &format!("connecting to {}:{}", config.host, config.port),
        encoder,
        "",
    );
    let mut stream = connect_with_retry(config, state, app, encoder)?;
    stream
        .set_nodelay(true)
        .map_err(|err| format!("set TCP_NODELAY failed: {}", err))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|err| format!("set TCP read timeout failed: {}", err))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|err| format!("set TCP write timeout failed: {}", err))?;

    update_status(
        state,
        app,
        true,
        "performing protocol handshake",
        encoder,
        "",
    );
    protocol_handshake(&mut stream, config, command)?;
    stream
        .set_read_timeout(None)
        .map_err(|err| format!("clear TCP read timeout failed: {}", err))?;
    stream
        .set_write_timeout(None)
        .map_err(|err| format!("clear TCP write timeout failed: {}", err))?;

    update_status(state, app, true, "starting ffmpeg", encoder, "");
    let mut child = hidden_command(&command[0])
        .args(&command[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("ffmpeg start failed: {}", err))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "ffmpeg stdout unavailable".to_string())?;
    {
        let mut guard = child_slot
            .lock()
            .map_err(|_| "child lock poisoned".to_string())?;
        *guard = Some(child);
    }

    update_status(state, app, true, "streaming", encoder, "");
    let start = Instant::now();
    let mut last_emit = Instant::now();
    let mut seq = 0u32;
    let mut window_max_packet_bytes = 0u64;
    let mut max_packet_bytes = 0u64;
    let mut max_keyframe_bytes = 0u64;
    let mut max_delta_frame_bytes = 0u64;
    let mut ffmpeg_reads = 0u64;
    let mut ffmpeg_read_bytes = 0u64;
    let mut max_read_gap_ms = 0.0;
    let mut last_read: Option<Instant> = None;
    let bitrate_kbps = parse_size_to_kbits(&config.bitrate);
    let send_pacer = SendPacer::new(config.send_pacing, bitrate_kbps, config.fps);
    let sender_queue = Arc::new(SenderQueue::new(2));
    let sender_metrics = Arc::new(Mutex::new(SenderMetrics::default()));
    let sender_handle = {
        let queue = sender_queue.clone();
        let metrics = sender_metrics.clone();
        let stop = stop.clone();
        thread::spawn(move || sender_loop(stream, queue, metrics, stop, send_pacer))
    };
    let mut last_sent_vcl_packets = 0u64;
    let mut last_sent_bytes = 0u64;
    let mut access_unit_parser = HevcAccessUnitParser::default();
    let mut read_buffer = [0u8; 256 * 1024];
    let mut log_file = open_stream_log();
    write_stream_log(
        &mut log_file,
        &format!(
            "START encoder={} bitrate={} bufsize={} gop={} fps={} capture={} scale={} pacing={} command={}",
            encoder,
            config.bitrate,
            config.bufsize,
            config.gop,
            config.fps,
            config.capture_backend,
            config.scale,
            config.send_pacing,
            quote_command(command)
        ),
    );

    while !stop.load(Ordering::SeqCst) {
        let read = stdout
            .read(&mut read_buffer)
            .map_err(|err| format!("ffmpeg read failed: {}", err))?;
        if read == 0 {
            break;
        }
        let now = Instant::now();
        if let Some(previous_read) = last_read {
            let read_gap_ms = now.duration_since(previous_read).as_secs_f64() * 1000.0;
            max_read_gap_ms = f64::max(max_read_gap_ms, read_gap_ms);
        }
        last_read = Some(now);
        ffmpeg_reads += 1;
        ffmpeg_read_bytes += read as u64;

        let packets_to_send = access_unit_parser.push(&read_buffer[..read]);

        for packet in packets_to_send {
            let packet_bytes = packet.payload.len() as u64;
            let is_keyframe = packet.flags & protocol::FLAG_KEYFRAME != 0;
            let mut packet = packet;
            packet.sequence = seq;
            packet.timestamp_us = start.elapsed().as_micros() as u64;
            seq = seq.wrapping_add(1);
            window_max_packet_bytes = window_max_packet_bytes.max(packet_bytes);
            max_packet_bytes = max_packet_bytes.max(packet_bytes);
            if is_keyframe {
                max_keyframe_bytes = max_keyframe_bytes.max(packet_bytes);
            } else if packet.flags & protocol::FLAG_VCL != 0 {
                max_delta_frame_bytes = max_delta_frame_bytes.max(packet_bytes);
            }
            sender_queue.push(packet);
        }

        if last_emit.elapsed() >= Duration::from_millis(500) {
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let window_elapsed = last_emit.elapsed().as_secs_f64().max(0.001);
            let sender = sender_metrics
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            let (queue_depth, host_dropped_packets) = sender_queue.metrics();
            let window_vcl_packets = sender
                .sent_vcl_packets
                .saturating_sub(last_sent_vcl_packets);
            let window_bytes = sender.sent_bytes.saturating_sub(last_sent_bytes);
            let mut guard = state
                .lock()
                .map_err(|_| "state lock poisoned".to_string())?;
            guard.stats.running = true;
            guard.stats.status = "streaming".to_string();
            guard.stats.encoder = encoder.to_string();
            guard.stats.packets = sender.sent_packets;
            guard.stats.vcl_packets = sender.sent_vcl_packets;
            guard.stats.keyframe_packets = sender.sent_keyframe_packets;
            guard.stats.bytes = sender.sent_bytes;
            guard.stats.fps = sender.sent_vcl_packets as f64 / elapsed;
            guard.stats.mbps = sender.sent_bytes as f64 * 8.0 / elapsed / 1_000_000.0;
            guard.stats.current_fps = window_vcl_packets as f64 / window_elapsed;
            guard.stats.current_mbps = window_bytes as f64 * 8.0 / window_elapsed / 1_000_000.0;
            guard.stats.current_max_packet_bytes = window_max_packet_bytes;
            guard.stats.max_packet_bytes = max_packet_bytes;
            guard.stats.max_keyframe_bytes = max_keyframe_bytes;
            guard.stats.max_delta_frame_bytes = max_delta_frame_bytes;
            guard.stats.ffmpeg_reads = ffmpeg_reads;
            guard.stats.ffmpeg_read_bytes = ffmpeg_read_bytes;
            guard.stats.parser_buffer_bytes = access_unit_parser.buffer_len() as u64;
            guard.stats.max_read_gap_ms = max_read_gap_ms;
            guard.stats.socket_write_blocked_events = sender.socket_write_blocked_events;
            guard.stats.socket_write_blocked_ms = sender.socket_write_blocked_ms;
            guard.stats.max_socket_write_ms = sender.max_socket_write_ms;
            guard.stats.paced_packets = sender.paced_packets;
            guard.stats.paced_sleep_ms = sender.paced_sleep_ms;
            guard.stats.max_packet_send_ms = sender.max_packet_send_ms;
            guard.stats.sender_queue_depth = queue_depth;
            guard.stats.host_dropped_packets = host_dropped_packets;
            if !sender.last_error.is_empty() {
                guard.stats.last_error = sender.last_error.clone();
            }
            guard.stats.elapsed_seconds = elapsed;
            let stats = guard.stats.clone();
            let _ = app.emit("stream-stats", stats.clone());
            write_stats_log(&mut log_file, &stats);
            last_emit = Instant::now();
            last_sent_vcl_packets = sender.sent_vcl_packets;
            last_sent_bytes = sender.sent_bytes;
            window_max_packet_bytes = 0;
        }
    }
    sender_queue.close();
    let _ = sender_handle.join();

    if let Ok(mut guard) = child_slot.lock() {
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
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
                    &format!(
                        "waiting for receiver on {}:{} ({}/30)",
                        config.host, config.port, attempt
                    ),
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

fn update_status(
    state: &Arc<Mutex<AppStateInner>>,
    app: &AppHandle,
    running: bool,
    status: &str,
    encoder: &str,
    error: &str,
) {
    if let Ok(mut guard) = state.lock() {
        guard.stats.running = running;
        guard.stats.status = status.to_string();
        guard.stats.encoder = encoder.to_string();
        guard.stats.last_error = error.to_string();
        let _ = app.emit("stream-stats", guard.stats.clone());
    }
}

fn open_stream_log() -> Option<std::fs::File> {
    let path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("tablet2screen-stream.log");
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn write_stream_log(log_file: &mut Option<std::fs::File>, message: &str) {
    let Some(file) = log_file.as_mut() else {
        return;
    };
    let _ = writeln!(file, "{:.3} {}", timestamp_seconds(), message);
}

fn write_stats_log(log_file: &mut Option<std::fs::File>, stats: &StreamStats) {
    write_stream_log(
        log_file,
        &format!(
            "STAT elapsed={:.3} fps={:.2} mbps={:.2} current_fps={:.2} current_mbps={:.2} packets={} vcl={} keyframes={} max_packet={} window_max_packet={} max_keyframe={} max_delta={} reads={} read_bytes={} parser_buffer={} max_read_gap_ms={:.1} socket_stalls={} socket_stall_ms={:.1} max_socket_ms={:.1} paced={} paced_sleep_ms={:.1} max_send_ms={:.1} queue={} host_dropped={}",
            stats.elapsed_seconds,
            stats.fps,
            stats.mbps,
            stats.current_fps,
            stats.current_mbps,
            stats.packets,
            stats.vcl_packets,
            stats.keyframe_packets,
            stats.max_packet_bytes,
            stats.current_max_packet_bytes,
            stats.max_keyframe_bytes,
            stats.max_delta_frame_bytes,
            stats.ffmpeg_reads,
            stats.ffmpeg_read_bytes,
            stats.parser_buffer_bytes,
            stats.max_read_gap_ms,
            stats.socket_write_blocked_events,
            stats.socket_write_blocked_ms,
            stats.max_socket_write_ms,
            stats.paced_packets,
            stats.paced_sleep_ms,
            stats.max_packet_send_ms,
            stats.sender_queue_depth,
            stats.host_dropped_packets
        ),
    );
}

fn timestamp_seconds() -> f64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn protocol_handshake(
    stream: &mut TcpStream,
    config: &StreamConfig,
    command: &[String],
) -> Result<(), String> {
    protocol::write_message(
        stream,
        &protocol::Message {
            message_type: protocol::TYPE_HELLO,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload: protocol::hello_payload(),
        },
    )?;
    protocol::expect_type(protocol::read_message(stream)?, protocol::TYPE_HELLO_ACK)?;

    let (width, height) = output_dimensions(config, command);
    protocol::write_message(
        stream,
        &protocol::Message {
            message_type: protocol::TYPE_VIDEO_CONFIG,
            flags: 0,
            sequence: 1,
            timestamp_us: 0,
            payload: protocol::video_config_payload(
                width.min(u16::MAX as u32) as u16,
                height.min(u16::MAX as u32) as u16,
                config.fps.min(u16::MAX as u32) as u16,
                parse_size_to_kbits(&config.bitrate),
                config.gop.min(u16::MAX as u32) as u16,
            ),
        },
    )?;
    protocol::expect_type(
        protocol::read_message(stream)?,
        protocol::TYPE_VIDEO_CONFIG_ACK,
    )?;
    Ok(())
}

fn output_dimensions(config: &StreamConfig, command: &[String]) -> (u32, u32) {
    let scale = config.scale.trim();
    if let Some((width, height)) = scale.split_once(':') {
        if let (Ok(width), Ok(height)) = (width.parse::<u32>(), height.parse::<u32>()) {
            if width > 0 && height > 0 {
                return (width, height);
            }
        }
    }

    for window in command.windows(2) {
        if window[0] == "-video_size" {
            if let Some((width, height)) = window[1].split_once('x') {
                if let (Ok(width), Ok(height)) = (width.parse::<u32>(), height.parse::<u32>()) {
                    return (width, height);
                }
            }
        }
        if window[0] == "-i" {
            if let Some((_, value)) = window[1].split_once("video_size=") {
                let value = value.split(':').next().unwrap_or(value);
                if let Some((width, height)) = value.split_once('x') {
                    if let (Ok(width), Ok(height)) = (width.parse::<u32>(), height.parse::<u32>()) {
                        return (width, height);
                    }
                }
            }
            if let Some((_, value)) = window[1].split_once("width=") {
                let width_value = value.split(':').next().unwrap_or(value);
                if let Some((_, value)) = window[1].split_once("height=") {
                    let height_value = value.split(':').next().unwrap_or(value);
                    if let (Ok(width), Ok(height)) =
                        (width_value.parse::<u32>(), height_value.parse::<u32>())
                    {
                        return (width, height);
                    }
                }
            }
        }
    }
    (1920, 1080)
}

fn send_packet(
    stream: &mut TcpStream,
    sequence: u32,
    timestamp_us: u64,
    flags: u16,
    payload: &[u8],
    pacer: &SendPacer,
) -> Result<SendReport, String> {
    if !pacer.enabled {
        protocol::write_message_parts(
            stream,
            protocol::TYPE_VIDEO_PACKET,
            flags,
            sequence,
            timestamp_us,
            payload,
        )?;
        return Ok(SendReport::default());
    }

    let header = protocol::message_header(
        protocol::TYPE_VIDEO_PACKET,
        flags,
        sequence,
        timestamp_us,
        payload.len(),
    )?;

    stream
        .write_all(&header)
        .map_err(|err| format!("socket header write failed: {}", err))?;

    let started = Instant::now();
    let mut sent = 0usize;
    let mut report = SendReport {
        paced: true,
        ..SendReport::default()
    };

    for chunk in payload.chunks(pacer.chunk_bytes) {
        stream
            .write_all(chunk)
            .map_err(|err| format!("socket payload write failed: {}", err))?;
        sent += chunk.len();

        if sent <= pacer.burst_bytes {
            continue;
        }

        let paced_bytes = sent - pacer.burst_bytes;
        let target_elapsed = Duration::from_secs_f64(paced_bytes as f64 / pacer.bytes_per_second);
        let actual_elapsed = started.elapsed();
        if target_elapsed > actual_elapsed {
            let sleep_for = target_elapsed - actual_elapsed;
            thread::sleep(sleep_for);
            report.sleep_ms += sleep_for.as_secs_f64() * 1000.0;
        }
    }

    Ok(report)
}

#[derive(Clone, Copy)]
struct SendPacer {
    enabled: bool,
    bytes_per_second: f64,
    burst_bytes: usize,
    chunk_bytes: usize,
}

impl SendPacer {
    fn new(enabled: bool, bitrate_kbps: u32, fps: u32) -> Self {
        let bytes_per_second = (bitrate_kbps.max(1) as f64 * 1000.0) / 8.0;
        let frame_budget = bytes_per_second / fps.max(1) as f64;
        Self {
            enabled,
            bytes_per_second,
            // Allow about one frame budget immediately, then pace oversized frames.
            burst_bytes: frame_budget.max(32_768.0) as usize,
            chunk_bytes: 32 * 1024,
        }
    }
}

#[derive(Default)]
struct SendReport {
    paced: bool,
    sleep_ms: f64,
}

#[derive(Default, Clone)]
struct SenderMetrics {
    sent_packets: u64,
    sent_vcl_packets: u64,
    sent_keyframe_packets: u64,
    sent_bytes: u64,
    socket_write_blocked_events: u64,
    socket_write_blocked_ms: f64,
    max_socket_write_ms: f64,
    paced_packets: u64,
    paced_sleep_ms: f64,
    max_packet_send_ms: f64,
    last_error: String,
}

fn sender_loop(
    mut stream: TcpStream,
    queue: Arc<SenderQueue>,
    metrics: Arc<Mutex<SenderMetrics>>,
    stop: Arc<AtomicBool>,
    pacer: SendPacer,
) {
    while let Some(packet) = queue.pop(&stop) {
        let write_start = Instant::now();
        let result = send_packet(
            &mut stream,
            packet.sequence,
            packet.timestamp_us,
            packet.flags,
            &packet.payload,
            &pacer,
        );
        let write_ms = write_start.elapsed().as_secs_f64() * 1000.0;

        match result {
            Ok(send_report) => {
                if let Ok(mut guard) = metrics.lock() {
                    guard.sent_packets += 1;
                    guard.sent_bytes += packet.payload.len() as u64;
                    if packet.flags & protocol::FLAG_VCL != 0 {
                        guard.sent_vcl_packets += 1;
                    }
                    if packet.flags & protocol::FLAG_KEYFRAME != 0 {
                        guard.sent_keyframe_packets += 1;
                    }
                    if write_ms >= 10.0 {
                        guard.socket_write_blocked_events += 1;
                        guard.socket_write_blocked_ms += write_ms;
                    }
                    guard.max_socket_write_ms = guard.max_socket_write_ms.max(write_ms);
                    guard.max_packet_send_ms = guard.max_packet_send_ms.max(write_ms);
                    if send_report.paced {
                        guard.paced_packets += 1;
                        guard.paced_sleep_ms += send_report.sleep_ms;
                    }
                }
            }
            Err(error) => {
                if let Ok(mut guard) = metrics.lock() {
                    guard.last_error = error;
                }
                stop.store(true, Ordering::SeqCst);
                break;
            }
        }
    }

    let _ = protocol::write_message(
        &mut stream,
        &protocol::Message {
            message_type: protocol::TYPE_STOP,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload: Vec::new(),
        },
    );
}

fn flags_for_nal(nal: &[u8]) -> u16 {
    let nal_type = nal_type(nal);
    let mut flags = 0;
    if matches!(nal_type, 32 | 33 | 34) {
        flags |= protocol::FLAG_CONFIG_NAL;
    }
    if nal_type <= 31 {
        flags |= protocol::FLAG_VCL;
    }
    if matches!(nal_type, 16..=21) {
        flags |= protocol::FLAG_KEYFRAME;
    }
    if flags & protocol::FLAG_VCL != 0 && flags & protocol::FLAG_KEYFRAME == 0 {
        flags |= protocol::FLAG_DROPPABLE;
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

    fn buffer_len(&self) -> usize {
        self.buffer.len()
    }
}

struct EncodedPacket {
    sequence: u32,
    timestamp_us: u64,
    flags: u16,
    payload: Vec<u8>,
}

#[derive(Default)]
struct SenderQueueInner {
    packets: VecDeque<EncodedPacket>,
    dropped_packets: u64,
    closed: bool,
}

struct SenderQueue {
    inner: Mutex<SenderQueueInner>,
    cv: Condvar,
    capacity: usize,
}

impl SenderQueue {
    fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(SenderQueueInner::default()),
            cv: Condvar::new(),
            capacity,
        }
    }

    fn push(&self, packet: EncodedPacket) {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        while guard.packets.len() >= self.capacity {
            if drop_stale_sender_packet(&mut guard.packets) {
                guard.dropped_packets += 1;
            } else {
                break;
            }
        }

        guard.packets.push_back(packet);
        self.cv.notify_one();
    }

    fn pop(&self, stop: &AtomicBool) -> Option<EncodedPacket> {
        let mut guard = self.inner.lock().ok()?;
        loop {
            if let Some(packet) = guard.packets.pop_front() {
                return Some(packet);
            }
            if guard.closed || stop.load(Ordering::SeqCst) {
                return None;
            }
            guard = self.cv.wait(guard).ok()?;
        }
    }

    fn close(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.closed = true;
        }
        self.cv.notify_all();
    }

    fn metrics(&self) -> (u64, u64) {
        self.inner
            .lock()
            .map(|guard| (guard.packets.len() as u64, guard.dropped_packets))
            .unwrap_or((0, 0))
    }
}

fn drop_stale_sender_packet(packets: &mut VecDeque<EncodedPacket>) -> bool {
    if let Some(index) = packets
        .iter()
        .position(|packet| packet.flags & protocol::FLAG_DROPPABLE != 0)
    {
        packets.remove(index);
        return true;
    }

    if let Some(index) = packets
        .iter()
        .position(|packet| packet.flags & protocol::FLAG_CONFIG_NAL == 0)
    {
        packets.remove(index);
        return true;
    }

    false
}

#[derive(Default)]
struct HevcAccessUnitParser {
    annex_b: AnnexBParser,
    pending: Vec<u8>,
    pending_flags: u16,
    pending_has_vcl: bool,
}

impl HevcAccessUnitParser {
    fn push(&mut self, data: &[u8]) -> Vec<EncodedPacket> {
        let mut out = Vec::new();
        for nal in self.annex_b.push(data) {
            let nal_type = nal_type(&nal);
            let flags = flags_for_nal(&nal);
            let starts_new_picture =
                nal_type <= 31 && is_first_slice_segment(&nal) && self.pending_has_vcl;

            if starts_new_picture {
                self.flush_pending(&mut out);
            }

            self.pending_flags |= flags;
            if nal_type <= 31 {
                self.pending_has_vcl = true;
            }
            self.pending.extend_from_slice(&nal);
        }
        out
    }

    fn buffer_len(&self) -> usize {
        self.annex_b.buffer_len() + self.pending.len()
    }

    fn flush_pending(&mut self, out: &mut Vec<EncodedPacket>) {
        if self.pending.is_empty() || !self.pending_has_vcl {
            return;
        }
        out.push(EncodedPacket {
            sequence: 0,
            timestamp_us: 0,
            flags: self.pending_flags,
            payload: std::mem::take(&mut self.pending),
        });
        self.pending_flags = 0;
        self.pending_has_vcl = false;
    }
}

fn is_first_slice_segment(nal: &[u8]) -> bool {
    let offset = if nal.starts_with(&[0, 0, 0, 1]) {
        4
    } else if nal.starts_with(&[0, 0, 1]) {
        3
    } else {
        return false;
    };
    nal.len() > offset + 2 && (nal[offset + 2] & 0x80) != 0
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
