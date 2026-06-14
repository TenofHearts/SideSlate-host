#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::mem::size_of;
use std::net::TcpStream;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR,
    MONITORINFOEXW,
};

mod protocol;

const DEFAULT_HDC: &str =
    r"D:\Program\Huawei\DevEco Studio\sdk\default\openharmony\toolchains\hdc.exe";
const DEFAULT_FFMPEG: &str = r"D:\Program\ffmpeg-8.1.1\bin\ffmpeg.exe";
const NATIVE_TARGET_SCALE: &str = "2800:1840";
const NATIVE_TARGET_BITRATE: &str = "35M";
const NATIVE_TARGET_BUFSIZE: &str = "2M";
const NATIVE_TARGET_GOP_60: u32 = 15;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Serialize)]
struct DisplayInfo {
    id: usize,
    name: String,
    device_string: String,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    primary: bool,
    virtual_display: bool,
    hmonitor: u64,
    dxgi_adapter_idx: Option<u32>,
    dxgi_output_idx: Option<u32>,
    dxgi_adapter_name: String,
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
    resync_events: u64,
    resync_dropped_access_units: u64,
    bottleneck: String,
    effective_capture_backend: String,
    receiver_running: bool,
    receiver_decoder_started: bool,
    receiver_surface_ready: bool,
    receiver_packets: u64,
    receiver_bytes: u64,
    receiver_queued_inputs: u64,
    receiver_rendered_outputs: u64,
    receiver_dropped_packets: u64,
    receiver_sequence_gaps: u64,
    receiver_config_packets: u64,
    receiver_keyframes: u64,
    receiver_last_sequence: u32,
    receiver_queue_depth: u32,
    receiver_stream_width: i32,
    receiver_stream_height: i32,
    receiver_stream_fps: i32,
    receiver_last_error: i32,
    receiver_receive_mbps: f64,
    receiver_input_fps: f64,
    receiver_render_fps: f64,
    receiver_drop_fps: f64,
    receiver_max_receive_gap_ms: f64,
    receiver_max_input_gap_ms: f64,
    receiver_max_render_gap_ms: f64,
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
            resync_events: 0,
            resync_dropped_access_units: 0,
            bottleneck: "idle".to_string(),
            effective_capture_backend: String::new(),
            receiver_running: false,
            receiver_decoder_started: false,
            receiver_surface_ready: false,
            receiver_packets: 0,
            receiver_bytes: 0,
            receiver_queued_inputs: 0,
            receiver_rendered_outputs: 0,
            receiver_dropped_packets: 0,
            receiver_sequence_gaps: 0,
            receiver_config_packets: 0,
            receiver_keyframes: 0,
            receiver_last_sequence: 0,
            receiver_queue_depth: 0,
            receiver_stream_width: 0,
            receiver_stream_height: 0,
            receiver_stream_fps: 0,
            receiver_last_error: 0,
            receiver_receive_mbps: 0.0,
            receiver_input_fps: 0.0,
            receiver_render_fps: 0.0,
            receiver_drop_fps: 0.0,
            receiver_max_receive_gap_ms: 0.0,
            receiver_max_input_gap_ms: 0.0,
            receiver_max_render_gap_ms: 0.0,
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
    let config = effective_stream_config(config, &display, &encoder);
    let command = build_ffmpeg_command(&config, &display, &encoder)?;
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
            effective_capture_backend: config.capture_backend.clone(),
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
            display,
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
            let device_string = display_device_string(&name);
            displays.push(DisplayInfo {
                id: displays.len(),
                name,
                virtual_display: is_likely_virtual_display(&device_string),
                device_string,
                left: rect.left,
                top: rect.top,
                width: rect.right - rect.left,
                height: rect.bottom - rect.top,
                primary: info.monitorInfo.dwFlags & 1 == 1,
                hmonitor: monitor.0 as u64,
                dxgi_adapter_idx: None,
                dxgi_output_idx: None,
                dxgi_adapter_name: String::new(),
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
    attach_dxgi_outputs(&mut displays);
    Ok(displays)
}

#[derive(Clone)]
struct DxgiOutputInfo {
    adapter_idx: u32,
    output_idx: u32,
    device_name: String,
    adapter_name: String,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

fn attach_dxgi_outputs(displays: &mut [DisplayInfo]) {
    let outputs = enumerate_dxgi_outputs();
    for display in displays {
        if let Some(output) = outputs
            .iter()
            .find(|output| dxgi_output_matches(display, output))
        {
            display.dxgi_adapter_idx = Some(output.adapter_idx);
            display.dxgi_output_idx = Some(output.output_idx);
            display.dxgi_adapter_name = output.adapter_name.clone();
        }
    }
}

fn enumerate_dxgi_outputs() -> Vec<DxgiOutputInfo> {
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut outputs = Vec::new();
    for adapter_idx in 0..32 {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(adapter_idx) }) else {
            break;
        };
        outputs.extend(enumerate_adapter_outputs(adapter_idx, &adapter));
    }
    outputs
}

fn enumerate_adapter_outputs(adapter_idx: u32, adapter: &IDXGIAdapter1) -> Vec<DxgiOutputInfo> {
    let adapter_name = unsafe { adapter.GetDesc1() }
        .map(|desc| utf16_field_to_string(&desc.Description))
        .unwrap_or_default();
    let mut outputs = Vec::new();
    for output_idx in 0..32 {
        let Ok(output) = (unsafe { adapter.EnumOutputs(output_idx) }) else {
            break;
        };
        let Ok(desc) = (unsafe { output.GetDesc() }) else {
            continue;
        };
        let rect = desc.DesktopCoordinates;
        outputs.push(DxgiOutputInfo {
            adapter_idx,
            output_idx,
            device_name: utf16_field_to_string(&desc.DeviceName),
            adapter_name: adapter_name.clone(),
            left: rect.left,
            top: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        });
    }
    outputs
}

fn dxgi_output_matches(display: &DisplayInfo, output: &DxgiOutputInfo) -> bool {
    if display.name == output.device_name {
        return true;
    }
    display.left == output.left
        && display.top == output.top
        && display.width == output.width
        && display.height == output.height
}

fn display_device_string(device_name: &str) -> String {
    [
        display_adapter_string(device_name),
        display_monitor_string(device_name),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" / ")
}

fn display_adapter_string(device_name: &str) -> Option<String> {
    for index in 0..32 {
        let mut device = DISPLAY_DEVICEW::default();
        device.cb = size_of::<DISPLAY_DEVICEW>() as u32;
        let ok = unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) };
        if !ok.as_bool() {
            break;
        }

        let adapter_name = utf16_field_to_string(&device.DeviceName);
        if adapter_name == device_name {
            return Some(display_device_description(&device));
        }
    }
    None
}

fn display_monitor_string(device_name: &str) -> Option<String> {
    let mut wide_name = device_name.encode_utf16().collect::<Vec<_>>();
    wide_name.push(0);

    let mut device = DISPLAY_DEVICEW::default();
    device.cb = size_of::<DISPLAY_DEVICEW>() as u32;
    let ok = unsafe { EnumDisplayDevicesW(PCWSTR(wide_name.as_ptr()), 0, &mut device, 0) };
    if !ok.as_bool() {
        return None;
    }

    Some(display_device_description(&device))
}

fn display_device_description(device: &DISPLAY_DEVICEW) -> String {
    [
        utf16_field_to_string(&device.DeviceString),
        utf16_field_to_string(&device.DeviceID),
        utf16_field_to_string(&device.DeviceKey),
    ]
    .into_iter()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

fn utf16_field_to_string(field: &[u16]) -> String {
    let end = field.iter().position(|ch| *ch == 0).unwrap_or(field.len());
    String::from_utf16_lossy(&field[..end])
}

fn is_likely_virtual_display(device_string: &str) -> bool {
    let lower = device_string.to_ascii_lowercase();
    [
        "parsec",
        "virtual",
        "vdisplay",
        "indirect",
        "idd",
        "spacedesk",
        "mirage",
        "usb display",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
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

fn effective_stream_config(
    mut config: StreamConfig,
    display: &DisplayInfo,
    encoder: &str,
) -> StreamConfig {
    config = apply_native_target_profile(config, encoder);
    let max_recovery_gop = recovery_gop_for_resolution(&config, display);
    if config.gop > max_recovery_gop {
        config.gop = max_recovery_gop;
    }
    if display.virtual_display {
        if config.fps > 60 {
            config.fps = 60;
        }
    }
    config
}

fn apply_native_target_profile(mut config: StreamConfig, encoder: &str) -> StreamConfig {
    if config.scale.trim() != NATIVE_TARGET_SCALE {
        return config;
    }

    match encoder {
        "hevc_nvenc" if config.encoder == "auto" || looks_like_native_default_profile(&config) => {
            config.fps = 60;
            config.bitrate = NATIVE_TARGET_BITRATE.to_string();
            config.bufsize = NATIVE_TARGET_BUFSIZE.to_string();
            config.gop = NATIVE_TARGET_GOP_60;
        }
        "hevc_qsv" if config.encoder == "auto" || looks_like_native_default_profile(&config) => {
            config.fps = 60;
            config.bitrate = NATIVE_TARGET_BITRATE.to_string();
            config.bufsize = NATIVE_TARGET_BUFSIZE.to_string();
            config.gop = NATIVE_TARGET_GOP_60;
        }
        "libx265" if config.encoder == "auto" || looks_like_native_default_profile(&config) => {
            config.fps = 60;
            config.bitrate = NATIVE_TARGET_BITRATE.to_string();
            config.bufsize = NATIVE_TARGET_BUFSIZE.to_string();
            config.gop = NATIVE_TARGET_GOP_60;
        }
        _ => {}
    }
    config
}

fn looks_like_native_default_profile(config: &StreamConfig) -> bool {
    config.scale.trim() == NATIVE_TARGET_SCALE
        && config.fps == 60
        && config.gop == NATIVE_TARGET_GOP_60
        && matches!(
            config.bitrate.trim().to_ascii_uppercase().as_str(),
            "35M" | "55M" | "70M" | "80M"
        )
}

fn recovery_gop_for_resolution(config: &StreamConfig, display: &DisplayInfo) -> u32 {
    let (width, height) = requested_output_dimensions(config, display);
    let pixels = width.saturating_mul(height);
    let fps = config.fps.max(1);
    if pixels <= 2800 * 1840 {
        fps.saturating_add(3) / 4
    } else {
        fps.saturating_add(2) / 3
    }
    .max(1)
}

fn requested_output_dimensions(config: &StreamConfig, display: &DisplayInfo) -> (u32, u32) {
    parse_scale(&config.scale)
        .unwrap_or_else(|| (display.width.max(1) as u32, display.height.max(1) as u32))
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
    match config.capture_backend.as_str() {
        "ddagrab_zero_copy" => matches!(encoder, "hevc_nvenc" | "hevc_qsv"),
        "ddagrab" => matches!(encoder, "hevc_nvenc" | "hevc_qsv"),
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

fn ddagrab_device(display: &DisplayInfo) -> Result<(u32, u32), String> {
    let adapter_idx = display.dxgi_adapter_idx.ok_or_else(|| {
        format!(
            "selected display {} ({}, {}x{} at {},{}) is not mapped to a DXGI adapter; try GDI fallback or reconnect the display",
            display.id,
            display.name,
            display.width,
            display.height,
            display.left,
            display.top
        )
    })?;
    let output_idx = display.dxgi_output_idx.ok_or_else(|| {
        format!(
            "selected display {} ({}, {}x{} at {},{}) is not mapped to a DXGI output; try GDI fallback or reconnect the display",
            display.id,
            display.name,
            display.width,
            display.height,
            display.left,
            display.top
        )
    })?;
    Ok((adapter_idx, output_idx))
}

fn ddagrab_input(config: &StreamConfig, display: &DisplayInfo) -> Result<String, String> {
    let (_, output_idx) = ddagrab_device(display)?;
    Ok(format!(
        "ddagrab=output_idx={}:framerate={}:draw_mouse=1:dup_frames=1:video_size={}x{}:output_fmt=bgra:allow_fallback=1",
        output_idx, config.fps, display.width, display.height
    ))
}

fn build_ffmpeg_command(
    config: &StreamConfig,
    display: &DisplayInfo,
    encoder: &str,
) -> Result<Vec<String>, String> {
    let capture_backend = config.capture_backend.clone();
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

    match capture_backend.as_str() {
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
                "gfxcapture=hmonitor={}:max_framerate={}:capture_cursor=1:display_border=0:width={}:height={}:output_fmt=bgra",
                display.hmonitor, config.fps, display.width, display.height
            ),
        ]),
        _ if is_dxgi_capture(&capture_backend) => {
            let (adapter_idx, _) = ddagrab_device(display)?;
            command.extend([
                "-init_hw_device".into(),
                format!("d3d11va=t2s_dda:{}", adapter_idx),
            ]);
            if encoder == "hevc_qsv" {
                command.extend([
                    "-init_hw_device".into(),
                    "qsv=t2s_qsv@t2s_dda".into(),
                ]);
            }
            command.extend([
                "-filter_hw_device".into(),
                "t2s_dda".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                ddagrab_input(config, display)?,
            ]);
        }
        _ => command.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            ddagrab_input(config, display)?,
        ]),
    }

    let mut filters = Vec::new();
    let mut effective_config = config.clone();
    effective_config.capture_backend = capture_backend.clone();
    let gpu_resident_dxgi = use_gpu_resident_dxgi(&effective_config, encoder);
    if gpu_resident_dxgi && encoder == "hevc_qsv" {
        filters.push("hwmap=derive_device=qsv".to_string());
        if let Some((width, height)) = parse_scale(&config.scale) {
            filters.push(format!("scale_qsv=w={}:h={}:format=nv12", width, height));
        } else {
            filters.push("scale_qsv=format=nv12".to_string());
        }
    } else if gpu_resident_dxgi {
        if let Some((width, height)) = parse_scale(&config.scale) {
            filters.push(format!(
                "scale_d3d11=width={}:height={}:format=bgra",
                width, height
            ));
        }
    } else if capture_backend != "gdigrab" {
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
            "-strict_gop".into(),
            "1".into(),
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
        "-fps_mode".into(),
        "cfr".into(),
        "-r".into(),
        config.fps.to_string(),
        "-flush_packets".into(),
        "1".into(),
        "-f".into(),
        "hevc".into(),
        "pipe:1".into(),
    ]);
    Ok(command)
}

fn stream_thread(
    app: AppHandle,
    state: Arc<Mutex<AppStateInner>>,
    stop: Arc<AtomicBool>,
    child_slot: Arc<Mutex<Option<Child>>>,
    config: StreamConfig,
    display: DisplayInfo,
    command: Vec<String>,
    encoder: String,
) {
    let result = run_stream_loop(
        &app,
        &state,
        &stop,
        &child_slot,
        &config,
        &display,
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
    display: &DisplayInfo,
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
    let mut receiver_stats_stream = stream
        .try_clone()
        .map_err(|err| format!("clone TCP stream for receiver stats failed: {}", err))?;
    receiver_stats_stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|err| format!("set receiver stats read timeout failed: {}", err))?;
    let receiver_metrics = Arc::new(Mutex::new(ReceiverMetrics::default()));
    let receiver_stats_handle = {
        let metrics = receiver_metrics.clone();
        let stop = stop.clone();
        thread::spawn(move || receiver_stats_loop(&mut receiver_stats_stream, metrics, stop))
    };

    update_status(state, app, true, "starting ffmpeg", encoder, "");
    let mut child = hidden_command(&command[0])
        .args(&command[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("ffmpeg start failed: {}", err))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "ffmpeg stdout unavailable".to_string())?;
    let (stdout_tx, stdout_rx) = mpsc::channel::<Result<Vec<u8>, String>>();
    let stdout_reader_handle = thread::spawn(move || {
        let mut stdout = stdout;
        let mut read_buffer = [0u8; 256 * 1024];
        loop {
            match stdout.read(&mut read_buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if stdout_tx.send(Ok(read_buffer[..read].to_vec())).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = stdout_tx.send(Err(format!("ffmpeg read failed: {}", err)));
                    break;
                }
            }
        }
    });
    let stderr_tail = Arc::new(Mutex::new(String::new()));
    let stderr_handle = child.stderr.take().map(|stderr| {
        let stderr_tail = stderr_tail.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut tail) = stderr_tail.lock() {
                    tail.push_str(&line);
                    tail.push('\n');
                    if tail.len() > 8192 {
                        while tail.len() > 8192 {
                            tail.remove(0);
                        }
                    }
                }
            }
        })
    });
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
    let mut resync_events = 0u64;
    let mut resync_dropped_access_units = 0u64;
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
    let mut last_socket_write_blocked_events = 0u64;
    let mut last_socket_write_blocked_ms = 0.0;
    let mut access_unit_parser = HevcAccessUnitParser::default();
    let frame_duration_us = 1_000_000u64 / config.fps.max(1) as u64;
    let mut video_frame_index = 0u64;
    let idle_flush_interval =
        Duration::from_millis((2000u64 / config.fps.max(1) as u64).clamp(50, 200));
    let stats_interval = Duration::from_millis(500);
    let mut log_file = open_stream_log();
    write_stream_log(
        &mut log_file,
        &format!(
            "START encoder={} bitrate={} bufsize={} gop={} fps={} capture={} display={} device=\"{}\" virtual={} dxgi_adapter={:?} dxgi_output={:?} dxgi_adapter_name=\"{}\" scale={} pacing={} command={}",
            encoder,
            config.bitrate,
            config.bufsize,
            config.gop,
            config.fps,
            config.capture_backend,
            display.name,
            display.device_string,
            display.virtual_display,
            display.dxgi_adapter_idx,
            display.dxgi_output_idx,
            display.dxgi_adapter_name,
            config.scale,
            config.send_pacing,
            quote_command(command)
        ),
    );

    let mut ffmpeg_stdout_eof = false;
    while !stop.load(Ordering::SeqCst) {
        let packets_to_send = match stdout_rx.recv_timeout(idle_flush_interval) {
            Ok(Ok(chunk)) => {
                let now = Instant::now();
                if let Some(previous_read) = last_read {
                    let read_gap_ms = now.duration_since(previous_read).as_secs_f64() * 1000.0;
                    max_read_gap_ms = f64::max(max_read_gap_ms, read_gap_ms);
                }
                last_read = Some(now);
                ffmpeg_reads += 1;
                ffmpeg_read_bytes += chunk.len() as u64;
                access_unit_parser.push(&chunk)
            }
            Ok(Err(error)) => return Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => access_unit_parser.flush_idle(),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                ffmpeg_stdout_eof = true;
                let packets = access_unit_parser.flush_idle();
                if packets.is_empty() {
                    break;
                }
                packets
            }
        };

        for packet in packets_to_send {
            let packet_bytes = packet.payload.len() as u64;
            let is_keyframe = packet.flags & protocol::FLAG_KEYFRAME != 0;
            let mut packet = packet;
            packet.sequence = seq;
            // Keep decoder PTS dense even if capture/encode emits nothing while the desktop is static.
            packet.timestamp_us = video_timestamp_us(video_frame_index, frame_duration_us);
            video_frame_index = video_frame_index.saturating_add(1);
            seq = seq.wrapping_add(1);
            window_max_packet_bytes = window_max_packet_bytes.max(packet_bytes);
            max_packet_bytes = max_packet_bytes.max(packet_bytes);
            if is_keyframe {
                max_keyframe_bytes = max_keyframe_bytes.max(packet_bytes);
            } else if packet.flags & protocol::FLAG_VCL != 0 {
                max_delta_frame_bytes = max_delta_frame_bytes.max(packet_bytes);
            }
            match sender_queue.push(packet) {
                QueuePushResult::Queued => {}
                QueuePushResult::Full(packet) => {
                    resync_events += 1;
                    match sender_queue.push_realtime(packet) {
                        QueueRealtimePushResult::Queued { dropped } => {
                            resync_dropped_access_units += dropped;
                        }
                        QueueRealtimePushResult::DroppedIncoming => {
                            resync_dropped_access_units += 1;
                        }
                        QueueRealtimePushResult::Closed => break,
                    }
                }
                QueuePushResult::Closed => break,
            }
        }

        if last_emit.elapsed() >= stats_interval {
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let window_elapsed = last_emit.elapsed().as_secs_f64().max(0.001);
            let sender = sender_metrics
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            let receiver = receiver_metrics
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            let (queue_depth, host_dropped_packets) = sender_queue.metrics();
            let window_vcl_packets = sender
                .sent_vcl_packets
                .saturating_sub(last_sent_vcl_packets);
            let window_bytes = sender.sent_bytes.saturating_sub(last_sent_bytes);
            let window_socket_write_blocked_events = sender
                .socket_write_blocked_events
                .saturating_sub(last_socket_write_blocked_events);
            let window_socket_write_blocked_ms =
                (sender.socket_write_blocked_ms - last_socket_write_blocked_ms).max(0.0);
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
            guard.stats.socket_write_blocked_events = window_socket_write_blocked_events;
            guard.stats.socket_write_blocked_ms = window_socket_write_blocked_ms;
            guard.stats.max_socket_write_ms = sender.max_socket_write_ms;
            guard.stats.paced_packets = sender.paced_packets;
            guard.stats.paced_sleep_ms = sender.paced_sleep_ms;
            guard.stats.max_packet_send_ms = sender.max_packet_send_ms;
            guard.stats.sender_queue_depth = queue_depth;
            guard.stats.host_dropped_packets = host_dropped_packets;
            guard.stats.resync_events = resync_events;
            guard.stats.resync_dropped_access_units = resync_dropped_access_units;
            guard.stats.effective_capture_backend = config.capture_backend.clone();
            apply_receiver_metrics(&mut guard.stats, &receiver);
            guard.stats.bottleneck = classify_bottleneck(&guard.stats, config);
            if !sender.last_error.is_empty() {
                guard.stats.last_error = sender.last_error.clone();
            } else if !receiver.last_error.is_empty() {
                guard.stats.last_error = receiver.last_error.clone();
            }
            guard.stats.elapsed_seconds = elapsed;
            let stats = guard.stats.clone();
            let _ = app.emit("stream-stats", stats.clone());
            write_stats_log(&mut log_file, &stats);
            last_emit = Instant::now();
            last_sent_vcl_packets = sender.sent_vcl_packets;
            last_sent_bytes = sender.sent_bytes;
            last_socket_write_blocked_events = sender.socket_write_blocked_events;
            last_socket_write_blocked_ms = sender.socket_write_blocked_ms;
            window_max_packet_bytes = 0;
        }
    }
    sender_queue.close();
    let _ = sender_handle.join();
    stop.store(true, Ordering::SeqCst);
    let _ = receiver_stats_handle.join();

    let mut ffmpeg_status_error = None;
    if let Ok(mut guard) = child_slot.lock() {
        if let Some(mut child) = guard.take() {
            if stop.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
            } else if ffmpeg_stdout_eof {
                match child.wait() {
                    Ok(status) if status.success() => {}
                    Ok(status) => {
                        ffmpeg_status_error = Some(format!("ffmpeg exited with {}", status));
                    }
                    Err(err) => {
                        ffmpeg_status_error = Some(format!("ffmpeg wait failed: {}", err));
                    }
                }
            } else {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }
    let _ = stdout_reader_handle.join();
    if let Some(error) = ffmpeg_status_error {
        let tail = stderr_tail
            .lock()
            .map(|tail| tail.trim().to_string())
            .unwrap_or_default();
        if tail.is_empty() {
            return Err(error);
        }
        return Err(format!("{}. Recent ffmpeg stderr:\n{}", error, tail));
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
            "STAT elapsed={:.3} bottleneck=\"{}\" host_fps={:.2} host_mbps={:.2} host_current_fps={:.2} host_current_mbps={:.2} host_packets={} host_vcl={} host_keyframes={} max_packet={} window_max_packet={} max_keyframe={} max_delta={} ffmpeg_reads={} ffmpeg_read_bytes={} parser_buffer={} host_max_read_gap_ms={:.1} socket_stalls={} socket_stall_ms={:.1} max_socket_ms={:.1} paced={} paced_sleep_ms={:.1} max_send_ms={:.1} host_queue={} host_dropped={} resync_events={} resync_dropped={} capture={} receiver_running={} receiver_surface={} receiver_decoder={} receiver_packets={} receiver_bytes={} receiver_mbps={:.2} receiver_inputs={} receiver_input_fps={:.2} receiver_rendered={} receiver_render_fps={:.2} receiver_dropped={} receiver_drop_fps={:.2} receiver_queue={} receiver_seq_gaps={} receiver_config={} receiver_keyframes={} receiver_stream={}x{}@{} receiver_max_rx_gap_ms={:.1} receiver_max_input_gap_ms={:.1} receiver_max_render_gap_ms={:.1} receiver_last_error={}",
            stats.elapsed_seconds,
            stats.bottleneck,
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
            stats.host_dropped_packets,
            stats.resync_events,
            stats.resync_dropped_access_units,
            stats.effective_capture_backend,
            stats.receiver_running,
            stats.receiver_surface_ready,
            stats.receiver_decoder_started,
            stats.receiver_packets,
            stats.receiver_bytes,
            stats.receiver_receive_mbps,
            stats.receiver_queued_inputs,
            stats.receiver_input_fps,
            stats.receiver_rendered_outputs,
            stats.receiver_render_fps,
            stats.receiver_dropped_packets,
            stats.receiver_drop_fps,
            stats.receiver_queue_depth,
            stats.receiver_sequence_gaps,
            stats.receiver_config_packets,
            stats.receiver_keyframes,
            stats.receiver_stream_width,
            stats.receiver_stream_height,
            stats.receiver_stream_fps,
            stats.receiver_max_receive_gap_ms,
            stats.receiver_max_input_gap_ms,
            stats.receiver_max_render_gap_ms,
            stats.receiver_last_error
        ),
    );
}

fn classify_bottleneck(stats: &StreamStats, config: &StreamConfig) -> String {
    let frame_ms = 1000.0 / config.fps.max(1) as f64;
    let receiver_has_stats = stats.receiver_packets > 0 || stats.receiver_rendered_outputs > 0;

    if stats.socket_write_blocked_events > 0
        && (stats.max_socket_write_ms > frame_ms * 2.0 || stats.sender_queue_depth > 0)
    {
        return "transport backpressure: TCP/HDC write is slower than encoder output".to_string();
    }
    if stats.host_dropped_packets > 0 || stats.resync_events > 0 {
        return "host queue overflow: sender fell behind and had to resync".to_string();
    }
    if stats.max_read_gap_ms > frame_ms * 3.0 && stats.current_fps < config.fps as f64 * 0.85 {
        return "capture/encoder jitter: FFmpeg output has long frame gaps".to_string();
    }
    if receiver_has_stats
        && stats.receiver_receive_mbps < stats.current_mbps * 0.75
        && stats.current_mbps > 1.0
    {
        return "receiver transport: tablet receive rate trails host send rate".to_string();
    }
    if receiver_has_stats
        && stats.receiver_input_fps < stats.current_fps * 0.75
        && stats.current_fps > 5.0
    {
        return "tablet decoder input: receiver is not feeding frames fast enough".to_string();
    }
    if receiver_has_stats
        && stats.receiver_render_fps < stats.receiver_input_fps * 0.75
        && stats.receiver_input_fps > 5.0
    {
        return "tablet render/decode: outputs are slower than decoder inputs".to_string();
    }
    if stats.receiver_sequence_gaps > 0 || stats.receiver_dropped_packets > 0 {
        return "receiver recovery: packet gaps or stale frames reached tablet".to_string();
    }
    "no obvious bottleneck in current sample".to_string()
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
    pacer: &mut SendPacer,
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

    let mut report = SendReport {
        paced: true,
        ..SendReport::default()
    };
    let started = Instant::now();
    let mut sent = 0usize;

    for chunk in payload.chunks(pacer.chunk_bytes) {
        stream
            .write_all(chunk)
            .map_err(|err| format!("socket payload write failed: {}", err))?;
        sent += chunk.len();

        let target_elapsed = pacer.target_delay_for_sent(sent);
        let actual_elapsed = started.elapsed();
        if target_elapsed > actual_elapsed {
            let sleep_for = target_elapsed - actual_elapsed;
            thread::sleep(sleep_for);
            report.sleep_ms += sleep_for.as_secs_f64() * 1000.0;
        }
    }

    Ok(report)
}

fn video_timestamp_us(frame_index: u64, frame_duration_us: u64) -> u64 {
    frame_index.saturating_mul(frame_duration_us)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> StreamConfig {
        StreamConfig {
            hdc_path: "hdc".to_string(),
            ffmpeg_path: "ffmpeg".to_string(),
            encoder: "hevc_nvenc".to_string(),
            capture_backend: "ddagrab".to_string(),
            display_id: 1,
            fps: 60,
            bitrate: NATIVE_TARGET_BITRATE.to_string(),
            bufsize: NATIVE_TARGET_BUFSIZE.to_string(),
            gop: NATIVE_TARGET_GOP_60,
            scale: NATIVE_TARGET_SCALE.to_string(),
            send_pacing: true,
            host: "127.0.0.1".to_string(),
            port: 5000,
        }
    }

    fn virtual_display() -> DisplayInfo {
        DisplayInfo {
            id: 1,
            name: "\\\\.\\DISPLAY2".to_string(),
            device_string: "Parsec Virtual Display".to_string(),
            left: 2560,
            top: 0,
            width: 3840,
            height: 2160,
            primary: false,
            virtual_display: true,
            hmonitor: 1234,
            dxgi_adapter_idx: Some(2),
            dxgi_output_idx: Some(0),
            dxgi_adapter_name: "Parsec Virtual Adapter".to_string(),
        }
    }

    #[test]
    fn ddagrab_command_uses_dxgi_mapping_not_display_id() {
        let command = build_ffmpeg_command(&test_config(), &virtual_display(), "hevc_nvenc")
            .expect("command should build");

        assert!(command
            .windows(2)
            .any(|window| window[0] == "-init_hw_device" && window[1] == "d3d11va=t2s_dda:2"));
        assert!(command
            .windows(2)
            .any(|window| window[0] == "-i" && window[1].contains("ddagrab=output_idx=0:")));
        assert!(!command
            .windows(2)
            .any(|window| window[0] == "-i" && window[1].contains("ddagrab=output_idx=1:")));
    }

    #[test]
    fn output_dimensions_parse_full_ddagrab_video_size() {
        let mut config = test_config();
        config.scale.clear();
        let command = build_ffmpeg_command(&config, &virtual_display(), "hevc_nvenc")
            .expect("command should build");

        assert_eq!(output_dimensions(&config, &command), (3840, 2160));
    }

    #[test]
    fn ffmpeg_command_forces_constant_rate_output() {
        let config = test_config();
        let command = build_ffmpeg_command(&config, &virtual_display(), "hevc_nvenc")
            .expect("command should build");

        assert!(command
            .windows(2)
            .any(|window| window[0] == "-fps_mode" && window[1] == "cfr"));
        assert!(command
            .windows(2)
            .any(|window| window[0] == "-r" && window[1] == config.fps.to_string()));
        assert!(command
            .windows(2)
            .any(|window| window[0] == "-strict_gop" && window[1] == "1"));
    }

    #[test]
    fn qsv_dxgi_command_keeps_frames_on_gpu() {
        let config = test_config();
        let command =
            build_ffmpeg_command(&config, &virtual_display(), "hevc_qsv").expect("command");
        let filter = command
            .windows(2)
            .find_map(|window| (window[0] == "-vf").then_some(window[1].as_str()))
            .expect("filter chain should exist");

        assert!(command.windows(2).any(|window| {
            window[0] == "-init_hw_device" && window[1] == "qsv=t2s_qsv@t2s_dda"
        }));
        assert!(filter.contains("hwmap=derive_device=qsv"));
        assert!(filter.contains("scale_qsv=w=2800:h=1840:format=nv12"));
    }

    #[test]
    fn idle_flush_emits_final_complete_access_unit() {
        let mut parser = HevcAccessUnitParser::default();
        let idr_nal = [0, 0, 0, 1, 38, 1, 0x80, 0];

        assert!(parser.push(&idr_nal).is_empty());

        let packets = parser.flush_idle();
        assert_eq!(packets.len(), 1);
        assert!(packets[0].flags & protocol::FLAG_VCL != 0);
        assert!(packets[0].flags & protocol::FLAG_KEYFRAME != 0);
        assert_eq!(packets[0].payload, idr_nal);
        assert_eq!(parser.buffer_len(), 0);
    }

    #[test]
    fn parser_prepends_cached_config_to_headerless_keyframes() {
        let mut parser = HevcAccessUnitParser::default();
        let config_nals = [
            0, 0, 0, 1, 64, 1, 1, 0, 0, 0, 1, 66, 1, 2, 0, 0, 0, 1, 68, 1, 3,
        ];
        let idr_nal = [0, 0, 0, 1, 38, 1, 0x80, 4];
        let next_nal = [0, 0, 0, 1, 2, 1, 0x80, 5];
        let flush_nal = [0, 0, 0, 1, 2, 1, 0x80, 6];

        assert!(parser.push(&config_nals).is_empty());
        assert!(parser.push(&idr_nal).is_empty());
        assert!(parser.push(&next_nal).is_empty());
        let packets = parser.push(&flush_nal);

        assert_eq!(packets.len(), 1);
        assert!(packets[0].flags & protocol::FLAG_KEYFRAME != 0);
        assert!(packets[0].flags & protocol::FLAG_CONFIG_NAL != 0);
        assert!(packets[0].payload.starts_with(&config_nals));
        assert!(packets[0].payload.ends_with(&idr_nal));
    }

    #[test]
    fn video_timestamps_stay_dense_after_host_idle() {
        let frame_duration_us = 1_000_000 / 60;

        assert_eq!(video_timestamp_us(0, frame_duration_us), 0);
        assert_eq!(video_timestamp_us(1, frame_duration_us), 16_666);
        assert_eq!(video_timestamp_us(2, frame_duration_us), 33_332);
    }

    #[test]
    fn effective_config_bounds_1080p_recovery_to_quarter_second() {
        let mut config = test_config();
        config.gop = 60;
        config.scale = "1920:1080".to_string();

        let config = effective_stream_config(config, &virtual_display(), "hevc_qsv");

        assert_eq!(config.gop, 15);
    }

    #[test]
    fn effective_config_uses_longer_recovery_for_above_tablet_native_resolution() {
        let mut config = test_config();
        config.gop = 60;
        config.scale.clear();

        let config = effective_stream_config(config, &virtual_display(), "hevc_nvenc");

        assert_eq!(config.gop, 20);
    }

    #[test]
    fn native_target_profile_promotes_nvenc_machines_to_native_60() {
        let mut config = test_config();
        config.encoder = "auto".to_string();

        let config = effective_stream_config(config, &virtual_display(), "hevc_nvenc");

        assert_eq!(config.scale, NATIVE_TARGET_SCALE);
        assert_eq!(config.fps, 60);
        assert_eq!(config.bitrate, NATIVE_TARGET_BITRATE);
        assert_eq!(config.bufsize, NATIVE_TARGET_BUFSIZE);
        assert_eq!(config.gop, NATIVE_TARGET_GOP_60);
    }

    #[test]
    fn native_target_profile_keeps_qsv_machines_at_universal_target() {
        let mut config = test_config();
        config.encoder = "auto".to_string();

        let config = effective_stream_config(config, &virtual_display(), "hevc_qsv");

        assert_eq!(config.scale, NATIVE_TARGET_SCALE);
        assert_eq!(config.fps, 60);
        assert_eq!(config.bitrate, NATIVE_TARGET_BITRATE);
        assert_eq!(config.bufsize, NATIVE_TARGET_BUFSIZE);
        assert_eq!(config.gop, NATIVE_TARGET_GOP_60);
    }

    #[test]
    fn native_target_bitrate_uses_transport_safe_native_profile() {
        let pixels_1440p = 2560.0 * 1440.0;
        let pixels_4k = 3840.0 * 2160.0;
        let pixels_native = 2800.0 * 1840.0;
        let factor: f64 =
            ((pixels_native - pixels_1440p) / (pixels_4k - pixels_1440p)) * 20.0 + 20.0;
        let bitrate_mbps = (factor * 2.0).round() as u32;

        assert_eq!(bitrate_mbps, 53);
        assert_eq!(NATIVE_TARGET_BITRATE, "35M");
    }

    #[test]
    fn bottleneck_classifier_flags_transport_backpressure() {
        let mut stats = StreamStats {
            running: true,
            current_fps: 60.0,
            current_mbps: 45.0,
            max_socket_write_ms: 80.0,
            socket_write_blocked_events: 1,
            ..StreamStats::default()
        };
        let config = test_config();

        let bottleneck = classify_bottleneck(&stats, &config);
        assert!(bottleneck.contains("transport backpressure"));

        stats.socket_write_blocked_events = 0;
        stats.max_socket_write_ms = 0.0;
        stats.resync_events = 1;
        let bottleneck = classify_bottleneck(&stats, &config);
        assert!(bottleneck.contains("host queue overflow"));
    }

    #[test]
    fn receiver_stats_timeout_detection_handles_windows_localized_errors() {
        assert!(is_receiver_stats_timeout(
            "socket header read failed: connection attempt failed (os error 10060)"
        ));
        assert!(is_receiver_stats_timeout(
            "socket header read failed: 由于连接方在一段时间后没有正确答复 (os error 10060)"
        ));
        assert!(!is_receiver_stats_timeout("bad protocol magic"));
    }

    #[test]
    fn send_pacer_caps_added_latency_per_packet() {
        let pacer = SendPacer::new(true, 35_000, 60);

        assert_eq!(pacer.target_delay_for_sent(pacer.burst_bytes), Duration::ZERO);
        assert!(pacer.target_delay_for_sent(pacer.burst_bytes + 16 * 1024) > Duration::ZERO);
        assert_eq!(
            pacer.target_delay_for_sent(pacer.burst_bytes + 512 * 1024),
            Duration::from_millis(6)
        );
    }

    fn test_packet(sequence: u32, flags: u16) -> EncodedPacket {
        EncodedPacket {
            sequence,
            timestamp_us: 0,
            flags,
            payload: vec![sequence as u8],
        }
    }

    #[test]
    fn realtime_queue_drops_oldest_stale_delta_first() {
        let queue = SenderQueue::new(2);
        assert!(matches!(
            queue.push(test_packet(1, protocol::FLAG_VCL | protocol::FLAG_DROPPABLE)),
            QueuePushResult::Queued
        ));
        assert!(matches!(
            queue.push(test_packet(2, protocol::FLAG_VCL | protocol::FLAG_DROPPABLE)),
            QueuePushResult::Queued
        ));

        let result = queue.push_realtime(test_packet(
            3,
            protocol::FLAG_VCL | protocol::FLAG_DROPPABLE,
        ));

        assert!(matches!(result, QueueRealtimePushResult::Queued { dropped: 1 }));
        let guard = queue.inner.lock().expect("queue lock");
        let sequences = guard
            .packets
            .iter()
            .map(|packet| packet.sequence)
            .collect::<Vec<_>>();
        assert_eq!(sequences, vec![2, 3]);
        assert_eq!(guard.dropped_packets, 1);
    }

    #[test]
    fn realtime_queue_drops_incoming_delta_before_displacing_essential_packets() {
        let queue = SenderQueue::new(2);
        assert!(matches!(
            queue.push(test_packet(1, protocol::FLAG_CONFIG_NAL)),
            QueuePushResult::Queued
        ));
        assert!(matches!(
            queue.push(test_packet(2, protocol::FLAG_KEYFRAME | protocol::FLAG_VCL)),
            QueuePushResult::Queued
        ));

        let result = queue.push_realtime(test_packet(
            3,
            protocol::FLAG_VCL | protocol::FLAG_DROPPABLE,
        ));

        assert!(matches!(result, QueueRealtimePushResult::DroppedIncoming));
        let guard = queue.inner.lock().expect("queue lock");
        let sequences = guard
            .packets
            .iter()
            .map(|packet| packet.sequence)
            .collect::<Vec<_>>();
        assert_eq!(sequences, vec![1, 2]);
        assert_eq!(guard.dropped_packets, 1);
    }

    #[test]
    fn realtime_queue_lets_keyframe_replace_queued_essential_packets() {
        let queue = SenderQueue::new(2);
        assert!(matches!(
            queue.push(test_packet(1, protocol::FLAG_CONFIG_NAL)),
            QueuePushResult::Queued
        ));
        assert!(matches!(
            queue.push(test_packet(2, protocol::FLAG_KEYFRAME | protocol::FLAG_VCL)),
            QueuePushResult::Queued
        ));

        let result = queue.push_realtime(test_packet(
            3,
            protocol::FLAG_KEYFRAME | protocol::FLAG_VCL,
        ));

        assert!(matches!(result, QueueRealtimePushResult::Queued { dropped: 2 }));
        let guard = queue.inner.lock().expect("queue lock");
        let sequences = guard
            .packets
            .iter()
            .map(|packet| packet.sequence)
            .collect::<Vec<_>>();
        assert_eq!(sequences, vec![3]);
        assert_eq!(guard.dropped_packets, 2);
    }
}

struct SendPacer {
    enabled: bool,
    bytes_per_second: f64,
    burst_bytes: usize,
    chunk_bytes: usize,
    max_sleep_per_packet: Duration,
}

impl SendPacer {
    fn new(enabled: bool, bitrate_kbps: u32, fps: u32) -> Self {
        let bytes_per_second = (bitrate_kbps.max(1) as f64 * 1000.0) / 8.0;
        let frame_budget = bytes_per_second / fps.max(1) as f64;
        Self {
            enabled,
            bytes_per_second,
            // Allow about one frame budget immediately. Extra bytes get a small smoothing delay,
            // but never enough to become visible end-to-end latency.
            burst_bytes: frame_budget.clamp(32_768.0, 128_000.0) as usize,
            chunk_bytes: 16 * 1024,
            max_sleep_per_packet: Duration::from_millis(6),
        }
    }

    fn target_delay_for_sent(&self, sent: usize) -> Duration {
        if !self.enabled {
            return Duration::ZERO;
        }
        if sent <= self.burst_bytes {
            return Duration::ZERO;
        }

        let paced_bytes = sent - self.burst_bytes;
        Duration::from_secs_f64(paced_bytes as f64 / self.bytes_per_second)
            .min(self.max_sleep_per_packet)
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

#[derive(Default, Clone)]
struct ReceiverMetrics {
    stats: protocol::ReceiverStats,
    stats_messages: u64,
    last_error: String,
}

fn receiver_stats_loop(
    stream: &mut TcpStream,
    metrics: Arc<Mutex<ReceiverMetrics>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::SeqCst) {
        match protocol::read_message(stream) {
            Ok(message) if message.message_type == protocol::TYPE_STATS => {
                if let Some(stats) = protocol::parse_receiver_stats_payload(&message.payload) {
                    if let Ok(mut guard) = metrics.lock() {
                        guard.stats = stats;
                        guard.stats_messages += 1;
                        guard.last_error.clear();
                    }
                } else if let Ok(mut guard) = metrics.lock() {
                    guard.last_error = "receiver stats payload too short".to_string();
                }
            }
            Ok(message) if message.message_type == protocol::TYPE_KEYFRAME_REQUEST => {
                if let Ok(mut guard) = metrics.lock() {
                    guard.last_error = "receiver requested keyframe; host-side request handling is not implemented yet".to_string();
                }
            }
            Ok(message) => {
                if let Ok(mut guard) = metrics.lock() {
                    guard.last_error =
                        format!("unexpected receiver message type {}", message.message_type);
                }
            }
            Err(error) => {
                if stop.load(Ordering::SeqCst) || is_receiver_stats_timeout(&error) {
                    continue;
                }
                if let Ok(mut guard) = metrics.lock() {
                    guard.last_error = error;
                }
                break;
            }
        }
    }
}

fn is_receiver_stats_timeout(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("timed out")
        || lower.contains("would block")
        || lower.contains("temporarily unavailable")
        || lower.contains("10060")
        || error.contains("超时")
}

fn apply_receiver_metrics(stats: &mut StreamStats, receiver: &ReceiverMetrics) {
    stats.receiver_running = receiver.stats.running;
    stats.receiver_decoder_started = receiver.stats.decoder_started;
    stats.receiver_surface_ready = receiver.stats.surface_ready;
    stats.receiver_packets = receiver.stats.packets;
    stats.receiver_bytes = receiver.stats.bytes;
    stats.receiver_queued_inputs = receiver.stats.queued_inputs;
    stats.receiver_rendered_outputs = receiver.stats.rendered_outputs;
    stats.receiver_dropped_packets = receiver.stats.dropped_packets;
    stats.receiver_sequence_gaps = receiver.stats.sequence_gaps;
    stats.receiver_config_packets = receiver.stats.config_packets;
    stats.receiver_keyframes = receiver.stats.keyframes;
    stats.receiver_last_sequence = receiver.stats.last_sequence;
    stats.receiver_queue_depth = receiver.stats.queue_depth;
    stats.receiver_stream_width = receiver.stats.stream_width;
    stats.receiver_stream_height = receiver.stats.stream_height;
    stats.receiver_stream_fps = receiver.stats.stream_fps;
    stats.receiver_last_error = receiver.stats.last_error;
    stats.receiver_receive_mbps = receiver.stats.receive_mbps;
    stats.receiver_input_fps = receiver.stats.input_fps;
    stats.receiver_render_fps = receiver.stats.render_fps;
    stats.receiver_drop_fps = receiver.stats.drop_fps;
    stats.receiver_max_receive_gap_ms = receiver.stats.max_receive_gap_ms;
    stats.receiver_max_input_gap_ms = receiver.stats.max_input_gap_ms;
    stats.receiver_max_render_gap_ms = receiver.stats.max_render_gap_ms;
}

fn sender_loop(
    mut stream: TcpStream,
    queue: Arc<SenderQueue>,
    metrics: Arc<Mutex<SenderMetrics>>,
    stop: Arc<AtomicBool>,
    mut pacer: SendPacer,
) {
    while let Some(packet) = queue.pop(&stop) {
        let write_start = Instant::now();
        let result = send_packet(
            &mut stream,
            packet.sequence,
            packet.timestamp_us,
            packet.flags,
            &packet.payload,
            &mut pacer,
        );
        let write_ms = write_start.elapsed().as_secs_f64() * 1000.0;

        match result {
            Ok(send_report) => {
                let socket_write_ms = (write_ms - send_report.sleep_ms).max(0.0);
                if let Ok(mut guard) = metrics.lock() {
                    guard.sent_packets += 1;
                    guard.sent_bytes += packet.payload.len() as u64;
                    if packet.flags & protocol::FLAG_VCL != 0 {
                        guard.sent_vcl_packets += 1;
                    }
                    if packet.flags & protocol::FLAG_KEYFRAME != 0 {
                        guard.sent_keyframe_packets += 1;
                    }
                    if socket_write_ms >= 10.0 {
                        guard.socket_write_blocked_events += 1;
                        guard.socket_write_blocked_ms += socket_write_ms;
                    }
                    guard.max_socket_write_ms = guard.max_socket_write_ms.max(socket_write_ms);
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

    fn flush_tail(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let Some((first, first_len)) = find_start_code(&self.buffer, 0) else {
            self.buffer.clear();
            return out;
        };
        if first > 0 {
            self.buffer.drain(..first);
        }
        if self.buffer.len() > first_len {
            out.push(std::mem::take(&mut self.buffer));
        }
        out
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

enum QueuePushResult {
    Queued,
    Full(EncodedPacket),
    Closed,
}

enum QueueRealtimePushResult {
    Queued { dropped: u64 },
    DroppedIncoming,
    Closed,
}

impl SenderQueue {
    fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(SenderQueueInner::default()),
            cv: Condvar::new(),
            capacity,
        }
    }

    fn push(&self, packet: EncodedPacket) -> QueuePushResult {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return QueuePushResult::Closed,
        };

        if guard.closed {
            return QueuePushResult::Closed;
        }
        if guard.packets.len() >= self.capacity {
            return QueuePushResult::Full(packet);
        }

        guard.packets.push_back(packet);
        self.cv.notify_one();
        QueuePushResult::Queued
    }

    fn push_realtime(&self, packet: EncodedPacket) -> QueueRealtimePushResult {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return QueueRealtimePushResult::Closed,
        };

        if guard.closed {
            return QueueRealtimePushResult::Closed;
        }

        let mut dropped = 0u64;
        while guard.packets.len() >= self.capacity {
            if drop_stale_sender_packet(&mut guard.packets) {
                guard.dropped_packets += 1;
                dropped += 1;
                continue;
            }

            if packet.flags & protocol::FLAG_KEYFRAME != 0 {
                dropped += guard.packets.len() as u64;
                guard.dropped_packets += guard.packets.len() as u64;
                guard.packets.clear();
                break;
            }

            if packet.flags & protocol::FLAG_DROPPABLE != 0 {
                guard.dropped_packets += 1;
                return QueueRealtimePushResult::DroppedIncoming;
            }

            return QueueRealtimePushResult::DroppedIncoming;
        }

        guard.packets.push_back(packet);
        self.cv.notify_one();
        QueueRealtimePushResult::Queued { dropped }
    }

    fn pop(&self, stop: &AtomicBool) -> Option<EncodedPacket> {
        let mut guard = self.inner.lock().ok()?;
        loop {
            if let Some(packet) = guard.packets.pop_front() {
                self.cv.notify_one();
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

    false
}

#[derive(Default)]
struct HevcAccessUnitParser {
    annex_b: AnnexBParser,
    config_nals: Vec<u8>,
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

            if flags & protocol::FLAG_CONFIG_NAL != 0 {
                if nal_type == 32 {
                    self.config_nals.clear();
                }
                self.config_nals.extend_from_slice(&nal);
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

    fn flush_idle(&mut self) -> Vec<EncodedPacket> {
        let mut out = Vec::new();
        for nal in self.annex_b.flush_tail() {
            let flags = flags_for_nal(&nal);
            let nal_type = nal_type(&nal);
            if flags & protocol::FLAG_CONFIG_NAL != 0 {
                if nal_type == 32 {
                    self.config_nals.clear();
                }
                self.config_nals.extend_from_slice(&nal);
            }
            self.pending_flags |= flags;
            if nal_type <= 31 {
                self.pending_has_vcl = true;
            }
            self.pending.extend_from_slice(&nal);
        }
        self.flush_pending(&mut out);
        out
    }

    fn flush_pending(&mut self, out: &mut Vec<EncodedPacket>) {
        if self.pending.is_empty() || !self.pending_has_vcl {
            return;
        }
        let mut flags = self.pending_flags;
        let mut payload = std::mem::take(&mut self.pending);
        if flags & protocol::FLAG_KEYFRAME != 0
            && flags & protocol::FLAG_CONFIG_NAL == 0
            && !self.config_nals.is_empty()
        {
            let mut keyframe = Vec::with_capacity(self.config_nals.len() + payload.len());
            keyframe.extend_from_slice(&self.config_nals);
            keyframe.extend_from_slice(&payload);
            payload = keyframe;
            flags |= protocol::FLAG_CONFIG_NAL;
        }
        out.push(EncodedPacket {
            sequence: 0,
            timestamp_us: 0,
            flags,
            payload,
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
