use serde::Serialize;
use std::mem::ManuallyDrop;
use std::time::{Duration, Instant};
use windows::core::{Interface, PWSTR, VARIANT};
use windows::Win32::Foundation::{BOOL, HANDLE, HMODULE, POINT};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, ID3D11VideoContext,
    ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator,
    D3D11_BIND_RENDER_TARGET, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_USAGE_STAGING, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0,
    D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
    D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication,
    IDXGIResource, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_DESC, DXGI_OUTDUPL_FRAME_INFO,
    DXGI_OUTDUPL_POINTER_SHAPE_INFO, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR,
    DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME,
};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Media::MediaFoundation::{
    eAVEncCommonRateControlMode_CBR, CODECAPI_AVEncCommonBufferSize,
    CODECAPI_AVEncCommonLowLatency, CODECAPI_AVEncCommonMaxBitRate,
    CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode,
    CODECAPI_AVEncSliceControlMode, CODECAPI_AVEncSliceControlSize, CODECAPI_AVLowLatencyMode,
    ICodecAPI, IMFDXGIDeviceManager, IMFMediaBuffer, IMFMediaEventGenerator, IMFMediaType,
    IMFSample, IMFTransform, METransformHaveOutput, METransformNeedInput,
    MFCreateDXGIDeviceManager, MFCreateDXGISurfaceBuffer, MFCreateMediaType, MFCreateMemoryBuffer,
    MFCreateSample, MFMediaType_Video, MFShutdown, MFStartup, MFTEnumEx,
    MFT_FRIENDLY_NAME_Attribute, MFVideoFormat_HEVC, MFVideoFormat_NV12,
    MFVideoInterlace_Progressive, MFSTARTUP_FULL, MFT_CATEGORY_VIDEO_ENCODER,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_MESSAGE_SET_D3D_MANAGER, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFT_REGISTER_TYPE_INFO, MF_EVENT_FLAG_NO_WAIT,
    MF_E_NOTACCEPTING, MF_E_NO_EVENTS_AVAILABLE, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_LOW_LATENCY,
    MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_NALU_LENGTH_SET,
    MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, GetCursorInfo, GetIconInfo, GetSystemMetrics, CURSORINFO, CURSOR_SHOWING,
    DI_NORMAL, HICON, ICONINFO, SM_CXCURSOR, SM_CYCURSOR,
};

#[derive(Clone)]
pub struct NativeDisplayTarget {
    pub adapter_idx: u32,
    pub output_idx: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVideoProbe {
    pub capture_available: bool,
    pub capture_format: String,
    pub capture_width: u32,
    pub capture_height: u32,
    pub hardware_hevc_encoders: Vec<String>,
    pub hardware_hevc_encoder_instantiable: bool,
    pub hardware_hevc_encoder_configurable: bool,
    pub notes: Vec<String>,
}

pub fn probe_native_video(target: NativeDisplayTarget) -> Result<NativeVideoProbe, String> {
    let mut notes = Vec::new();
    let capture = match probe_dxgi_duplication(&target) {
        Ok(capture) => Some(capture),
        Err(error) => {
            notes.push(format!("DXGI duplication unavailable: {}", error));
            None
        }
    };
    let encoder_width = capture
        .as_ref()
        .map(|capture| capture.width)
        .unwrap_or(1920);
    let encoder_height = capture
        .as_ref()
        .map(|capture| capture.height)
        .unwrap_or(1080);
    let hardware_hevc_encoders =
        match enumerate_hardware_hevc_encoders(encoder_width, encoder_height, 60, 20_000_000) {
            Ok(encoders) => encoders,
            Err(error) => {
                notes.push(format!(
                    "Media Foundation HEVC encoder probe failed: {}",
                    error
                ));
                HardwareHevcEncoderProbe {
                    names: Vec::new(),
                    instantiable: false,
                    configurable: false,
                }
            }
        };

    Ok(NativeVideoProbe {
        capture_available: capture.is_some(),
        capture_format: capture
            .as_ref()
            .map(|capture| capture.format.clone())
            .unwrap_or_default(),
        capture_width: capture.as_ref().map(|capture| capture.width).unwrap_or(0),
        capture_height: capture.as_ref().map(|capture| capture.height).unwrap_or(0),
        hardware_hevc_encoder_instantiable: hardware_hevc_encoders.instantiable,
        hardware_hevc_encoder_configurable: hardware_hevc_encoders.configurable,
        hardware_hevc_encoders: hardware_hevc_encoders.names,
        notes,
    })
}

pub enum NativeEncodedVideoEvent {
    Bytes(Vec<u8>),
    Timeout,
    Ended,
}

pub struct NativeEncoderConfig {
    pub target: NativeDisplayTarget,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
}

const MF_HEVC_SLICE_CONTROL_MODE_BYTES: u32 = 1;
const MF_HEVC_SLICE_CONTROL_SIZE_BYTES: u32 = 96 * 1024;
const MF_HEVC_RATE_BUFFER_BYTES: u32 = 256 * 1024;

pub struct NativeHevcVideoSource {
    _session: MediaFoundationSession,
    duplication: IDXGIOutputDuplication,
    device_context: ID3D11DeviceContext,
    gpu_input: Option<GpuInputPath>,
    staging_texture: Option<ID3D11Texture2D>,
    capture_format: DXGI_FORMAT,
    encoder: IMFTransform,
    encoder_events: Option<IMFMediaEventGenerator>,
    encoder_needs_input: bool,
    encoder_has_output: bool,
    output_provides_samples: bool,
    output_buffer_size: u32,
    nalu_length_size: Option<usize>,
    sequence_header: Vec<u8>,
    sequence_header_sent: bool,
    width: u32,
    height: u32,
    desktop_left: i32,
    desktop_top: i32,
    fps: u32,
    frame_duration: Duration,
    next_frame_due: Instant,
    frame_index: u64,
    last_nv12: Option<Vec<u8>>,
    pointer: PointerState,
    ended: bool,
}

#[derive(Default)]
struct PointerState {
    visible: bool,
    position: POINT,
    shape: Option<PointerShape>,
}

struct PointerShape {
    info: DXGI_OUTDUPL_POINTER_SHAPE_INFO,
    bytes: Vec<u8>,
}

impl NativeHevcVideoSource {
    pub fn start(config: NativeEncoderConfig) -> Result<Self, String> {
        let session = MediaFoundationSession::start()?;
        let capture = DxgiCaptureSession::start(&config.target)?;
        let width = config.width.max(1);
        let height = config.height.max(1);
        if capture.desc.ModeDesc.Width != width || capture.desc.ModeDesc.Height != height {
            return Err(format!(
                "native_mf currently requires capture size {}x{} to match display size {}x{}",
                width, height, capture.desc.ModeDesc.Width, capture.desc.ModeDesc.Height
            ));
        }
        if !matches!(
            capture.desc.ModeDesc.Format,
            DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM
        ) {
            return Err(format!(
                "native_mf unsupported DXGI capture format {:?}",
                capture.desc.ModeDesc.Format
            ));
        }

        let gpu_input: Result<GpuInputPath, String> =
            Err("native GPU input path is disabled after black-frame output on Arc/MF".to_string());
        let (gpu_input, encoder, staging_texture) = match gpu_input {
            Ok(gpu_input) => {
                match create_configured_hevc_encoder(
                    width,
                    height,
                    config.fps,
                    config.bitrate,
                    Some(&gpu_input.device_manager),
                ) {
                    Ok(encoder) => (Some(gpu_input), encoder, None),
                    Err(_) => {
                        let staging_texture = create_staging_texture(
                            &capture.device,
                            width,
                            height,
                            capture.desc.ModeDesc.Format,
                        )?;
                        let encoder = create_configured_hevc_encoder(
                            width,
                            height,
                            config.fps,
                            config.bitrate,
                            None,
                        )?;
                        (None, encoder, Some(staging_texture))
                    }
                }
            }
            Err(_) => {
                let staging_texture = create_staging_texture(
                    &capture.device,
                    width,
                    height,
                    capture.desc.ModeDesc.Format,
                )?;
                let encoder = create_configured_hevc_encoder(
                    width,
                    height,
                    config.fps,
                    config.bitrate,
                    None,
                )?;
                (None, encoder, Some(staging_texture))
            }
        };
        let output_info = unsafe { encoder.GetOutputStreamInfo(0) }
            .map_err(|error| format!("GetOutputStreamInfo failed: {}", error))?;
        let output_type = unsafe { encoder.GetOutputCurrentType(0) }
            .map_err(|error| format!("GetOutputCurrentType failed: {}", error))?;
        let nalu_length_size = unsafe { output_type.GetUINT32(&MF_NALU_LENGTH_SET) }
            .ok()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| (1..=4).contains(value));
        let sequence_header =
            media_type_blob(&output_type, &MF_MT_MPEG_SEQUENCE_HEADER).unwrap_or_default();
        let output_provides_samples =
            output_info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
        let output_buffer_size = output_info.cbSize.max(width.saturating_mul(height));
        let encoder_events = encoder.cast::<IMFMediaEventGenerator>().ok();
        let fps = config.fps.max(1);
        let frame_duration = Duration::from_micros(1_000_000u64 / fps as u64);
        let mut source = Self {
            _session: session,
            duplication: capture.duplication,
            device_context: capture.device_context,
            gpu_input,
            staging_texture,
            capture_format: capture.desc.ModeDesc.Format,
            encoder,
            encoder_needs_input: encoder_events.is_none(),
            encoder_has_output: false,
            encoder_events,
            output_provides_samples,
            output_buffer_size,
            nalu_length_size,
            sequence_header,
            sequence_header_sent: false,
            width,
            height,
            desktop_left: capture.desktop_left,
            desktop_top: capture.desktop_top,
            fps,
            frame_duration,
            next_frame_due: Instant::now(),
            frame_index: 0,
            last_nv12: None,
            pointer: PointerState::default(),
            ended: false,
        };
        source.pump_encoder_events()?;
        Ok(source)
    }

    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<NativeEncodedVideoEvent, String> {
        if self.ended {
            return Ok(NativeEncodedVideoEvent::Ended);
        }
        let now = Instant::now();
        if now < self.next_frame_due {
            let sleep_for = (self.next_frame_due - now).min(timeout);
            if !sleep_for.is_zero() {
                std::thread::sleep(sleep_for);
            }
            if Instant::now() < self.next_frame_due {
                return Ok(NativeEncodedVideoEvent::Timeout);
            }
        }

        let pending = self.drain_encoder_output()?;
        if !pending.is_empty() {
            return Ok(NativeEncodedVideoEvent::Bytes(pending));
        }

        let bytes = if self.gpu_input.is_some() {
            if !self.capture_gpu_if_updated()? {
                self.next_frame_due = Instant::now() + self.frame_duration;
                return Ok(NativeEncodedVideoEvent::Timeout);
            }
            self.encode_gpu_texture()?
        } else {
            if let Some(nv12) = self.capture_nv12_if_updated()? {
                self.last_nv12 = Some(nv12);
            }
            let Some(nv12) = self.last_nv12.clone() else {
                self.next_frame_due = Instant::now() + self.frame_duration;
                return Ok(NativeEncodedVideoEvent::Timeout);
            };
            self.encode_nv12(&nv12)?
        };
        self.frame_index = self.frame_index.saturating_add(1);
        self.next_frame_due += self.frame_duration;
        if bytes.is_empty() {
            Ok(NativeEncodedVideoEvent::Timeout)
        } else {
            Ok(NativeEncodedVideoEvent::Bytes(bytes))
        }
    }

    pub fn finish(mut self) -> Result<Option<String>, String> {
        self.ended = true;
        let _ = unsafe {
            self.encoder.ProcessMessage(
                windows::Win32::Media::MediaFoundation::MFT_MESSAGE_COMMAND_DRAIN,
                0,
            )
        };
        Ok(Some("native_mf".to_string()))
    }

    fn capture_nv12_if_updated(&mut self) -> Result<Option<Vec<u8>>, String> {
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        match unsafe {
            self.duplication
                .AcquireNextFrame(0, &mut frame_info, &mut resource)
        } {
            Ok(()) => {}
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(None),
            Err(error) => return Err(format!("native_mf AcquireNextFrame failed: {}", error)),
        }

        let result = (|| {
            self.update_pointer_from_frame(&frame_info)?;
            let resource = resource.ok_or_else(|| "DXGI frame had no resource".to_string())?;
            let texture: ID3D11Texture2D = resource
                .cast()
                .map_err(|error| format!("DXGI frame texture cast failed: {}", error))?;
            unsafe {
                self.device_context
                    .CopyResource(self.staging_texture.as_ref().unwrap(), &texture);
            }
            self.copy_staging_to_nv12()
        })();
        let release_result = unsafe { self.duplication.ReleaseFrame() }
            .map_err(|error| format!("native_mf ReleaseFrame failed: {}", error));
        match (result, release_result) {
            (Ok(nv12), Ok(())) => Ok(Some(nv12)),
            (Err(error), _) => Err(error),
            (_, Err(error)) => Err(error),
        }
    }

    fn copy_staging_to_nv12(&self) -> Result<Vec<u8>, String> {
        let staging_texture = self
            .staging_texture
            .as_ref()
            .ok_or_else(|| "native_mf CPU staging texture is not available".to_string())?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.device_context
                .Map(staging_texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(|error| format!("native_mf staging Map failed: {}", error))?;
        let frame = copy_bgra_frame_with_pointer(
            mapped.pData as *const u8,
            mapped.RowPitch as usize,
            self.width as usize,
            self.height as usize,
            &self.pointer,
        )?;
        let result = bgra_mapped_to_nv12(
            frame.as_ptr(),
            self.width as usize * 4,
            self.width as usize,
            self.height as usize,
            self.capture_format,
        );
        unsafe {
            self.device_context.Unmap(staging_texture, 0);
        }
        result
    }

    fn update_pointer_from_frame(
        &mut self,
        frame_info: &DXGI_OUTDUPL_FRAME_INFO,
    ) -> Result<(), String> {
        if frame_info.LastMouseUpdateTime != 0 {
            self.pointer.visible = frame_info.PointerPosition.Visible.as_bool();
            self.pointer.position = POINT {
                x: frame_info.PointerPosition.Position.x - self.desktop_left,
                y: frame_info.PointerPosition.Position.y - self.desktop_top,
            };
        }
        if frame_info.PointerShapeBufferSize == 0 {
            self.update_pointer_from_system_cursor();
            return Ok(());
        }

        let mut required = 0u32;
        let mut shape_info = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
        let mut bytes = vec![0u8; frame_info.PointerShapeBufferSize as usize];
        unsafe {
            self.duplication.GetFramePointerShape(
                bytes.len() as u32,
                bytes.as_mut_ptr().cast(),
                &mut required,
                &mut shape_info,
            )
        }
        .map_err(|error| format!("GetFramePointerShape failed: {}", error))?;
        bytes.truncate(required as usize);
        self.pointer.shape = Some(PointerShape {
            info: shape_info,
            bytes,
        });
        self.update_pointer_from_system_cursor();
        Ok(())
    }

    fn update_pointer_from_system_cursor(&mut self) {
        let mut cursor = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetCursorInfo(&mut cursor) }.is_err()
            || cursor.flags.0 & CURSOR_SHOWING.0 == 0
            || cursor.hCursor.0.is_null()
        {
            return;
        }
        self.pointer.visible = true;
        self.pointer.position = POINT {
            x: cursor.ptScreenPos.x - self.desktop_left,
            y: cursor.ptScreenPos.y - self.desktop_top,
        };
        if let Some(shape) = render_system_cursor_shape(HICON(cursor.hCursor.0)) {
            self.pointer.shape = Some(shape);
        }
    }

    fn capture_gpu_if_updated(&mut self) -> Result<bool, String> {
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        match unsafe {
            self.duplication
                .AcquireNextFrame(0, &mut frame_info, &mut resource)
        } {
            Ok(()) => {}
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
            Err(error) => return Err(format!("native_mf AcquireNextFrame failed: {}", error)),
        }

        let result = (|| {
            let resource = resource.ok_or_else(|| "DXGI frame had no resource".to_string())?;
            let texture: ID3D11Texture2D = resource
                .cast()
                .map_err(|error| format!("DXGI frame texture cast failed: {}", error))?;
            self.gpu_input
                .as_ref()
                .ok_or_else(|| "native_mf GPU input path is not available".to_string())?
                .convert_to_nv12(&texture, self.frame_index as u32)
        })();
        let release_result = unsafe { self.duplication.ReleaseFrame() }
            .map_err(|error| format!("native_mf ReleaseFrame failed: {}", error));
        match (result, release_result) {
            (Ok(()), Ok(())) => Ok(true),
            (Err(error), _) => Err(error),
            (_, Err(error)) => Err(error),
        }
    }

    fn encode_gpu_texture(&mut self) -> Result<Vec<u8>, String> {
        let sample = self
            .gpu_input
            .as_ref()
            .ok_or_else(|| "native_mf GPU input path is not available".to_string())?
            .sample(self.frame_index, self.fps)?;
        self.encode_sample(&sample)
    }

    fn encode_nv12(&mut self, nv12: &[u8]) -> Result<Vec<u8>, String> {
        let sample = nv12_sample(nv12, self.frame_index, self.fps, self.width, self.height)?;
        self.encode_sample(&sample)
    }

    fn encode_sample(&mut self, sample: &IMFSample) -> Result<Vec<u8>, String> {
        if !self.wait_for_encoder_input()? {
            return self.drain_encoder_output();
        }
        match unsafe { self.encoder.ProcessInput(0, sample, 0) } {
            Ok(()) => self.encoder_needs_input = false,
            Err(error) if error.code() == MF_E_NOTACCEPTING => {
                self.pump_encoder_events()?;
                let pending = self.drain_encoder_output()?;
                if !pending.is_empty() {
                    return Ok(pending);
                }
                if !self.wait_for_encoder_input()? {
                    return Ok(pending);
                }
                unsafe { self.encoder.ProcessInput(0, sample, 0) }
                    .map_err(|error| format!("native_mf ProcessInput failed: {}", error))?;
                self.encoder_needs_input = false;
            }
            Err(error) => return Err(format!("native_mf ProcessInput failed: {}", error)),
        }
        self.drain_encoder_output()
    }

    fn drain_encoder_output(&mut self) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        loop {
            self.pump_encoder_events()?;
            if self.encoder_events.is_some() && !self.encoder_has_output {
                break;
            }
            let output_sample = if self.output_provides_samples {
                None
            } else {
                Some(output_sample(self.output_buffer_size)?)
            };
            let mut output = MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(output_sample),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            };
            let mut status = 0u32;
            let result = unsafe {
                self.encoder
                    .ProcessOutput(0, std::slice::from_mut(&mut output), &mut status)
            };
            let sample = unsafe { ManuallyDrop::take(&mut output.pSample) };
            let events = unsafe { ManuallyDrop::take(&mut output.pEvents) };
            drop(events);
            match result {
                Ok(()) => {
                    self.encoder_has_output = false;
                    if let Some(sample) = sample {
                        let sample_bytes = sample_bytes(&sample)?;
                        bytes.extend(self.normalize_hevc_sample(&sample_bytes)?);
                    }
                }
                Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => break,
                Err(error) => return Err(format!("native_mf ProcessOutput failed: {}", error)),
            }
        }
        Ok(bytes)
    }

    fn wait_for_encoder_input(&mut self) -> Result<bool, String> {
        if self.encoder_events.is_none() {
            return Ok(true);
        }
        let deadline = Instant::now() + Duration::from_millis(3);
        while !self.encoder_needs_input {
            self.pump_encoder_events()?;
            if self.encoder_needs_input {
                break;
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::yield_now();
        }
        Ok(true)
    }

    fn pump_encoder_events(&mut self) -> Result<(), String> {
        let Some(events) = &self.encoder_events else {
            return Ok(());
        };
        loop {
            match unsafe { events.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
                Ok(event) => {
                    let status = unsafe { event.GetStatus() }
                        .map_err(|error| format!("MFT event status read failed: {}", error))?;
                    status
                        .ok()
                        .map_err(|error| format!("MFT async event failed: {}", error))?;
                    let event_type = unsafe { event.GetType() }
                        .map_err(|error| format!("MFT event type read failed: {}", error))?;
                    if event_type == METransformNeedInput.0 as u32 {
                        self.encoder_needs_input = true;
                    } else if event_type == METransformHaveOutput.0 as u32 {
                        self.encoder_has_output = true;
                    }
                }
                Err(error) if error.code() == MF_E_NO_EVENTS_AVAILABLE => break,
                Err(error) => return Err(format!("MFT GetEvent failed: {}", error)),
            }
        }
        Ok(())
    }

    fn normalize_hevc_sample(&mut self, sample: &[u8]) -> Result<Vec<u8>, String> {
        if sample.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = if has_start_code(sample) {
            sample.to_vec()
        } else if let Some(length_size) = self.nalu_length_size {
            length_prefixed_hevc_to_annex_b(sample, length_size)?
        } else {
            sample.to_vec()
        };
        if !self.sequence_header_sent && !self.sequence_header.is_empty() {
            let header = if has_start_code(&self.sequence_header) {
                self.sequence_header.clone()
            } else if let Some(length_size) = self.nalu_length_size {
                length_prefixed_hevc_to_annex_b(&self.sequence_header, length_size)?
            } else {
                self.sequence_header.clone()
            };
            if !header.is_empty() && !out.starts_with(&header) {
                let mut with_header = header;
                with_header.extend_from_slice(&out);
                out = with_header;
            }
            self.sequence_header_sent = true;
        }
        Ok(out)
    }
}

#[allow(dead_code)]
struct GpuInputPath {
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    nv12_texture: ID3D11Texture2D,
    device_manager: IMFDXGIDeviceManager,
}

#[allow(dead_code)]
impl GpuInputPath {
    fn new(
        device: &ID3D11Device,
        device_context: &ID3D11DeviceContext,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<Self, String> {
        let device_manager = create_dxgi_device_manager(device)?;
        let video_device: ID3D11VideoDevice = device
            .cast()
            .map_err(|error| format!("ID3D11VideoDevice cast failed: {}", error))?;
        let video_context: ID3D11VideoContext = device_context
            .cast()
            .map_err(|error| format!("ID3D11VideoContext cast failed: {}", error))?;
        let content_desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: fps.max(1),
                Denominator: 1,
            },
            InputWidth: width,
            InputHeight: height,
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: fps.max(1),
                Denominator: 1,
            },
            OutputWidth: width,
            OutputHeight: height,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };
        let enumerator = unsafe { video_device.CreateVideoProcessorEnumerator(&content_desc) }
            .map_err(|error| format!("CreateVideoProcessorEnumerator failed: {}", error))?;
        let processor = unsafe { video_device.CreateVideoProcessor(&enumerator, 0) }
            .map_err(|error| format!("CreateVideoProcessor failed: {}", error))?;
        let nv12_texture = create_default_texture(
            device,
            width,
            height,
            DXGI_FORMAT_NV12,
            D3D11_BIND_RENDER_TARGET.0 as u32,
        )?;
        Ok(Self {
            video_device,
            video_context,
            enumerator,
            processor,
            nv12_texture,
            device_manager,
        })
    }

    fn convert_to_nv12(&self, source: &ID3D11Texture2D, frame_index: u32) -> Result<(), String> {
        let input_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut input_view = None;
        unsafe {
            self.video_device.CreateVideoProcessorInputView(
                source,
                &self.enumerator,
                &input_desc,
                Some(&mut input_view),
            )
        }
        .map_err(|error| format!("CreateVideoProcessorInputView failed: {}", error))?;
        let input_view = input_view
            .ok_or_else(|| "CreateVideoProcessorInputView returned no view".to_string())?;

        let output_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        let mut output_view = None;
        unsafe {
            self.video_device.CreateVideoProcessorOutputView(
                &self.nv12_texture,
                &self.enumerator,
                &output_desc,
                Some(&mut output_view),
            )
        }
        .map_err(|error| format!("CreateVideoProcessorOutputView failed: {}", error))?;
        let output_view = output_view
            .ok_or_else(|| "CreateVideoProcessorOutputView returned no view".to_string())?;

        let stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: BOOL(1),
            OutputIndex: 0,
            InputFrameOrField: 0,
            PastFrames: 0,
            FutureFrames: 0,
            ppPastSurfaces: std::ptr::null_mut(),
            pInputSurface: ManuallyDrop::new(Some(input_view)),
            ppFutureSurfaces: std::ptr::null_mut(),
            ppPastSurfacesRight: std::ptr::null_mut(),
            pInputSurfaceRight: ManuallyDrop::new(None),
            ppFutureSurfacesRight: std::ptr::null_mut(),
        };
        let mut streams = [stream];
        unsafe {
            self.video_context.VideoProcessorBlt(
                &self.processor,
                &output_view,
                frame_index,
                &streams,
            )
        }
        .map_err(|error| format!("VideoProcessorBlt failed: {}", error))?;
        let _ = unsafe { ManuallyDrop::take(&mut streams[0].pInputSurface) };
        let _ = unsafe { ManuallyDrop::take(&mut streams[0].pInputSurfaceRight) };
        Ok(())
    }

    fn sample(&self, frame_index: u64, fps: u32) -> Result<IMFSample, String> {
        dxgi_texture_sample(&self.nv12_texture, frame_index, fps)
    }
}

struct DxgiCaptureSession {
    duplication: IDXGIOutputDuplication,
    device: ID3D11Device,
    device_context: ID3D11DeviceContext,
    desc: DXGI_OUTDUPL_DESC,
    desktop_left: i32,
    desktop_top: i32,
}

impl DxgiCaptureSession {
    fn start(target: &NativeDisplayTarget) -> Result<Self, String> {
        let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }
            .map_err(|error| format!("CreateDXGIFactory1 failed: {}", error))?;
        let adapter = unsafe { factory.EnumAdapters1(target.adapter_idx) }
            .map_err(|error| format!("EnumAdapters1({}) failed: {}", target.adapter_idx, error))?;
        let output = unsafe { adapter.EnumOutputs(target.output_idx) }
            .map_err(|error| format!("EnumOutputs({}) failed: {}", target.output_idx, error))?;
        let output_desc = unsafe { output.GetDesc() }
            .map_err(|error| format!("IDXGIOutput GetDesc failed: {}", error))?;
        let output1: IDXGIOutput1 = output
            .cast()
            .map_err(|error| format!("IDXGIOutput1 cast failed: {}", error))?;
        let (device, device_context) = create_d3d11_device_with_context(&adapter)?;
        let duplication = unsafe { output1.DuplicateOutput(&device) }
            .map_err(|error| format!("DuplicateOutput failed: {}", error))?;
        let desc = unsafe { duplication.GetDesc() };
        Ok(Self {
            duplication,
            device,
            device_context,
            desc,
            desktop_left: output_desc.DesktopCoordinates.left,
            desktop_top: output_desc.DesktopCoordinates.top,
        })
    }
}

struct DxgiCaptureProbe {
    format: String,
    width: u32,
    height: u32,
}

fn probe_dxgi_duplication(target: &NativeDisplayTarget) -> Result<DxgiCaptureProbe, String> {
    let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }
        .map_err(|error| format!("CreateDXGIFactory1 failed: {}", error))?;
    let adapter = unsafe { factory.EnumAdapters1(target.adapter_idx) }
        .map_err(|error| format!("EnumAdapters1({}) failed: {}", target.adapter_idx, error))?;
    let output = unsafe { adapter.EnumOutputs(target.output_idx) }
        .map_err(|error| format!("EnumOutputs({}) failed: {}", target.output_idx, error))?;
    let output1: IDXGIOutput1 = output
        .cast()
        .map_err(|error| format!("IDXGIOutput1 cast failed: {}", error))?;
    let device = create_d3d11_device(&adapter)?;
    let duplication = unsafe { output1.DuplicateOutput(&device) }
        .map_err(|error| format!("DuplicateOutput failed: {}", error))?;
    let desc: DXGI_OUTDUPL_DESC = unsafe { duplication.GetDesc() };
    let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
    let mut resource: Option<IDXGIResource> = None;
    match unsafe { duplication.AcquireNextFrame(0, &mut frame_info, &mut resource) } {
        Ok(()) => unsafe {
            duplication
                .ReleaseFrame()
                .map_err(|error| format!("ReleaseFrame failed: {}", error))?;
        },
        Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
        Err(error) => return Err(format!("AcquireNextFrame failed: {}", error)),
    }

    Ok(DxgiCaptureProbe {
        format: format!("{:?}", desc.ModeDesc.Format),
        width: desc.ModeDesc.Width,
        height: desc.ModeDesc.Height,
    })
}

fn create_d3d11_device(adapter: &IDXGIAdapter1) -> Result<ID3D11Device, String> {
    create_d3d11_device_with_context(adapter).map(|(device, _)| device)
}

fn create_d3d11_device_with_context(
    adapter: &IDXGIAdapter1,
) -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
    let mut device = None;
    let mut device_context = None;
    let mut selected_level = D3D_FEATURE_LEVEL::default();
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut selected_level),
            Some(&mut device_context),
        )
    }
    .map_err(|error| format!("D3D11CreateDevice failed: {}", error))?;
    Ok((
        device.ok_or_else(|| "D3D11CreateDevice returned no device".to_string())?,
        device_context.ok_or_else(|| "D3D11CreateDevice returned no device context".to_string())?,
    ))
}

struct MediaFoundationSession;

impl MediaFoundationSession {
    fn start() -> Result<Self, String> {
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }
            .map_err(|error| format!("MFStartup failed: {}", error))?;
        Ok(Self)
    }
}

impl Drop for MediaFoundationSession {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
    }
}

struct HardwareHevcEncoderProbe {
    names: Vec<String>,
    instantiable: bool,
    configurable: bool,
}

fn enumerate_hardware_hevc_encoders(
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
) -> Result<HardwareHevcEncoderProbe, String> {
    let _session = MediaFoundationSession::start()?;
    let output_type = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_HEVC,
    };
    let mut activates = std::ptr::null_mut();
    let mut count = 0u32;
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&output_type),
            &mut activates,
            &mut count,
        )
    }
    .map_err(|error| format!("MFTEnumEx failed: {}", error))?;

    let mut names = Vec::new();
    let mut instantiable = false;
    let mut configurable = false;
    if !activates.is_null() {
        let slice = unsafe { std::slice::from_raw_parts(activates, count as usize) };
        for activate in slice.iter().filter_map(|activate| activate.as_ref()) {
            if !instantiable {
                if let Ok(transform) = unsafe { activate.ActivateObject::<IMFTransform>() } {
                    instantiable = true;
                    configurable =
                        configure_hevc_encoder(&transform, width, height, fps, bitrate, None)
                            .is_ok();
                }
            }
            names
                .push(mft_friendly_name(activate).unwrap_or_else(|| "unnamed HEVC encoder".into()));
        }
        unsafe { CoTaskMemFree(Some(activates.cast())) };
    }
    Ok(HardwareHevcEncoderProbe {
        names,
        instantiable,
        configurable,
    })
}

fn create_configured_hevc_encoder(
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    device_manager: Option<&IMFDXGIDeviceManager>,
) -> Result<IMFTransform, String> {
    let output_type = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_HEVC,
    };
    let mut activates = std::ptr::null_mut();
    let mut count = 0u32;
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&output_type),
            &mut activates,
            &mut count,
        )
    }
    .map_err(|error| format!("MFTEnumEx failed: {}", error))?;

    let mut last_error = "no hardware HEVC encoder found".to_string();
    if !activates.is_null() {
        let slice = unsafe { std::slice::from_raw_parts(activates, count as usize) };
        for activate in slice.iter().filter_map(|activate| activate.as_ref()) {
            let name = mft_friendly_name(activate).unwrap_or_else(|| "unnamed HEVC encoder".into());
            match unsafe { activate.ActivateObject::<IMFTransform>() } {
                Ok(transform) => {
                    match configure_hevc_encoder(
                        &transform,
                        width,
                        height,
                        fps,
                        bitrate,
                        device_manager,
                    ) {
                        Ok(()) => {
                            unsafe { CoTaskMemFree(Some(activates.cast())) };
                            return Ok(transform);
                        }
                        Err(error) => last_error = format!("{}: {}", name, error),
                    }
                }
                Err(error) => last_error = format!("{} activation failed: {}", name, error),
            }
        }
        unsafe { CoTaskMemFree(Some(activates.cast())) };
    }
    Err(last_error)
}

fn configure_hevc_encoder(
    transform: &IMFTransform,
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    device_manager: Option<&IMFDXGIDeviceManager>,
) -> Result<(), String> {
    if let Ok(attributes) = unsafe { transform.GetAttributes() } {
        let _ = unsafe { attributes.SetUINT32(&MF_LOW_LATENCY, 1) };
        let _ = unsafe { attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1) };
    }
    configure_hevc_encoder_options(transform, bitrate);
    if let Some(device_manager) = device_manager {
        unsafe {
            transform.ProcessMessage(
                MFT_MESSAGE_SET_D3D_MANAGER,
                device_manager.as_raw() as usize,
            )
        }
        .map_err(|error| format!("MFT set D3D manager failed: {}", error))?;
    }

    let output_type = hevc_output_type(width, height, fps, bitrate)?;
    unsafe { transform.SetOutputType(0, &output_type, 0) }
        .map_err(|error| format!("SetOutputType(HEVC) failed: {}", error))?;

    let input_type = nv12_input_type(width, height, fps)?;
    unsafe { transform.SetInputType(0, &input_type, 0) }
        .map_err(|error| format!("SetInputType(NV12) failed: {}", error))?;

    unsafe { transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0) }
        .map_err(|error| format!("MFT begin streaming failed: {}", error))?;
    unsafe { transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0) }
        .map_err(|error| format!("MFT start of stream failed: {}", error))?;
    Ok(())
}

fn configure_hevc_encoder_options(transform: &IMFTransform, bitrate: u32) {
    let Ok(codec_api) = transform.cast::<ICodecAPI>() else {
        return;
    };
    let enabled = VARIANT::from(1u32);
    let rate_control = VARIANT::from(eAVEncCommonRateControlMode_CBR.0);
    let mean_bitrate = VARIANT::from(bitrate);
    let max_bitrate = VARIANT::from(bitrate);
    let buffer_size = VARIANT::from(MF_HEVC_RATE_BUFFER_BYTES);
    let mode = VARIANT::from(MF_HEVC_SLICE_CONTROL_MODE_BYTES);
    let size = VARIANT::from(MF_HEVC_SLICE_CONTROL_SIZE_BYTES);
    let _ = unsafe { codec_api.SetValue(&CODECAPI_AVLowLatencyMode, &enabled) };
    let _ = unsafe { codec_api.SetValue(&CODECAPI_AVEncCommonLowLatency, &enabled) };
    let _ = unsafe { codec_api.SetValue(&CODECAPI_AVEncCommonRateControlMode, &rate_control) };
    let _ = unsafe { codec_api.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &mean_bitrate) };
    let _ = unsafe { codec_api.SetValue(&CODECAPI_AVEncCommonMaxBitRate, &max_bitrate) };
    let _ = unsafe { codec_api.SetValue(&CODECAPI_AVEncCommonBufferSize, &buffer_size) };
    let _ = unsafe { codec_api.SetValue(&CODECAPI_AVEncSliceControlMode, &mode) };
    let _ = unsafe { codec_api.SetValue(&CODECAPI_AVEncSliceControlSize, &size) };
}

fn hevc_output_type(
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
) -> Result<IMFMediaType, String> {
    let media_type = unsafe { MFCreateMediaType() }
        .map_err(|error| format!("MFCreateMediaType(HEVC) failed: {}", error))?;
    set_video_type_common(&media_type, &MFVideoFormat_HEVC, width, height, fps)?;
    unsafe { media_type.SetUINT32(&MF_MT_AVG_BITRATE, bitrate) }
        .map_err(|error| format!("Set HEVC bitrate failed: {}", error))?;
    Ok(media_type)
}

fn nv12_input_type(width: u32, height: u32, fps: u32) -> Result<IMFMediaType, String> {
    let media_type = unsafe { MFCreateMediaType() }
        .map_err(|error| format!("MFCreateMediaType(NV12) failed: {}", error))?;
    set_video_type_common(&media_type, &MFVideoFormat_NV12, width, height, fps)?;
    Ok(media_type)
}

fn set_video_type_common(
    media_type: &IMFMediaType,
    subtype: &windows::core::GUID,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(), String> {
    unsafe { media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) }
        .map_err(|error| format!("Set major type failed: {}", error))?;
    unsafe { media_type.SetGUID(&MF_MT_SUBTYPE, subtype) }
        .map_err(|error| format!("Set subtype failed: {}", error))?;
    unsafe { media_type.SetUINT64(&MF_MT_FRAME_SIZE, pack_ratio(width, height)) }
        .map_err(|error| format!("Set frame size failed: {}", error))?;
    unsafe { media_type.SetUINT64(&MF_MT_FRAME_RATE, pack_ratio(fps, 1)) }
        .map_err(|error| format!("Set frame rate failed: {}", error))?;
    unsafe { media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_ratio(1, 1)) }
        .map_err(|error| format!("Set pixel aspect ratio failed: {}", error))?;
    unsafe { media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32) }
        .map_err(|error| format!("Set interlace mode failed: {}", error))?;
    Ok(())
}

fn pack_ratio(numerator: u32, denominator: u32) -> u64 {
    ((numerator as u64) << 32) | denominator as u64
}

fn create_staging_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
) -> Result<ID3D11Texture2D, String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut texture = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
        .map_err(|error| format!("CreateTexture2D(staging) failed: {}", error))?;
    texture.ok_or_else(|| "CreateTexture2D(staging) returned no texture".to_string())
}

#[allow(dead_code)]
fn create_default_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
    bind_flags: u32,
) -> Result<ID3D11Texture2D, String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: bind_flags,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
        .map_err(|error| format!("CreateTexture2D(default) failed: {}", error))?;
    texture.ok_or_else(|| "CreateTexture2D(default) returned no texture".to_string())
}

#[allow(dead_code)]
fn create_dxgi_device_manager(device: &ID3D11Device) -> Result<IMFDXGIDeviceManager, String> {
    let mut reset_token = 0u32;
    let mut manager = None;
    unsafe { MFCreateDXGIDeviceManager(&mut reset_token, &mut manager) }
        .map_err(|error| format!("MFCreateDXGIDeviceManager failed: {}", error))?;
    let manager =
        manager.ok_or_else(|| "MFCreateDXGIDeviceManager returned no manager".to_string())?;
    unsafe { manager.ResetDevice(device, reset_token) }
        .map_err(|error| format!("DXGI device manager ResetDevice failed: {}", error))?;
    Ok(manager)
}

fn bgra_mapped_to_nv12(
    data: *const u8,
    row_pitch: usize,
    width: usize,
    height: usize,
    format: DXGI_FORMAT,
) -> Result<Vec<u8>, String> {
    if data.is_null() {
        return Err("mapped staging texture has null data".to_string());
    }
    let y_len = width
        .checked_mul(height)
        .ok_or_else(|| "native_mf frame size overflow".to_string())?;
    let uv_len = y_len / 2;
    let mut nv12 = vec![0u8; y_len + uv_len];
    for y in 0..height {
        let row = unsafe { std::slice::from_raw_parts(data.add(y * row_pitch), width * 4) };
        for x in 0..width {
            let px = &row[x * 4..x * 4 + 4];
            let (r, g, b) = if format == DXGI_FORMAT_R8G8B8A8_UNORM {
                (px[0] as i32, px[1] as i32, px[2] as i32)
            } else {
                (px[2] as i32, px[1] as i32, px[0] as i32)
            };
            nv12[y * width + x] = clamp_u8(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16);
        }
    }
    let uv_base = y_len;
    for y in (0..height.saturating_sub(1)).step_by(2) {
        let row0 = unsafe { std::slice::from_raw_parts(data.add(y * row_pitch), width * 4) };
        let row1 = unsafe { std::slice::from_raw_parts(data.add((y + 1) * row_pitch), width * 4) };
        for x in (0..width.saturating_sub(1)).step_by(2) {
            let mut r_sum = 0i32;
            let mut g_sum = 0i32;
            let mut b_sum = 0i32;
            for px in [
                &row0[x * 4..x * 4 + 4],
                &row0[(x + 1) * 4..(x + 1) * 4 + 4],
                &row1[x * 4..x * 4 + 4],
                &row1[(x + 1) * 4..(x + 1) * 4 + 4],
            ] {
                if format == DXGI_FORMAT_R8G8B8A8_UNORM {
                    r_sum += px[0] as i32;
                    g_sum += px[1] as i32;
                    b_sum += px[2] as i32;
                } else {
                    r_sum += px[2] as i32;
                    g_sum += px[1] as i32;
                    b_sum += px[0] as i32;
                }
            }
            let r = r_sum / 4;
            let g = g_sum / 4;
            let b = b_sum / 4;
            let uv_index = uv_base + (y / 2) * width + x;
            nv12[uv_index] = clamp_u8(((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128);
            nv12[uv_index + 1] = clamp_u8(((112 * r - 94 * g - 18 * b + 128) >> 8) + 128);
        }
    }
    Ok(nv12)
}

fn copy_bgra_frame_with_pointer(
    data: *const u8,
    row_pitch: usize,
    width: usize,
    height: usize,
    pointer: &PointerState,
) -> Result<Vec<u8>, String> {
    if data.is_null() {
        return Err("mapped staging texture has null data".to_string());
    }
    let mut frame = vec![0u8; width * height * 4];
    for y in 0..height {
        let src = unsafe { std::slice::from_raw_parts(data.add(y * row_pitch), width * 4) };
        let dst = &mut frame[y * width * 4..(y + 1) * width * 4];
        dst.copy_from_slice(src);
    }
    overlay_pointer_bgra(&mut frame, width, height, pointer);
    Ok(frame)
}

fn overlay_pointer_bgra(frame: &mut [u8], width: usize, height: usize, pointer: &PointerState) {
    if !pointer.visible {
        return;
    }
    let Some(shape) = &pointer.shape else {
        return;
    };
    let shape_type = shape.info.Type;
    let shape_width = shape.info.Width as usize;
    let shape_height = if shape_type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32 {
        shape.info.Height as usize / 2
    } else {
        shape.info.Height as usize
    };
    let pitch = shape.info.Pitch as usize;
    if shape_width == 0 || shape_height == 0 || pitch == 0 {
        return;
    }

    let origin_x = pointer.position.x - shape.info.HotSpot.x;
    let origin_y = pointer.position.y - shape.info.HotSpot.y;
    if shape_type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32 {
        overlay_color_pointer_bgra(
            frame,
            width,
            height,
            &shape.bytes,
            pitch,
            shape_width,
            shape_height,
            origin_x,
            origin_y,
        );
    } else if shape_type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0 as u32 {
        overlay_masked_color_pointer_bgra(
            frame,
            width,
            height,
            &shape.bytes,
            pitch,
            shape_width,
            shape_height,
            origin_x,
            origin_y,
        );
    } else if shape_type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32 {
        overlay_monochrome_pointer_bgra(
            frame,
            width,
            height,
            &shape.bytes,
            pitch,
            shape_width,
            shape_height,
            origin_x,
            origin_y,
        );
    }
}

fn overlay_color_pointer_bgra(
    frame: &mut [u8],
    width: usize,
    height: usize,
    cursor: &[u8],
    pitch: usize,
    cursor_width: usize,
    cursor_height: usize,
    origin_x: i32,
    origin_y: i32,
) {
    for cy in 0..cursor_height {
        let y = origin_y + cy as i32;
        if y < 0 || y >= height as i32 {
            continue;
        }
        for cx in 0..cursor_width {
            let x = origin_x + cx as i32;
            if x < 0 || x >= width as i32 {
                continue;
            }
            let src = cy * pitch + cx * 4;
            if src + 4 > cursor.len() {
                continue;
            }
            let alpha = cursor[src + 3] as u32;
            if alpha == 0 {
                continue;
            }
            let dst = (y as usize * width + x as usize) * 4;
            if alpha == 255 {
                frame[dst..dst + 4].copy_from_slice(&cursor[src..src + 4]);
                frame[dst + 3] = 255;
            } else {
                for channel in 0..3 {
                    let foreground = cursor[src + channel] as u32;
                    let background = frame[dst + channel] as u32;
                    frame[dst + channel] =
                        ((foreground * alpha + background * (255 - alpha)) / 255) as u8;
                }
                frame[dst + 3] = 255;
            }
        }
    }
}

fn overlay_masked_color_pointer_bgra(
    frame: &mut [u8],
    width: usize,
    height: usize,
    cursor: &[u8],
    pitch: usize,
    cursor_width: usize,
    cursor_height: usize,
    origin_x: i32,
    origin_y: i32,
) {
    for cy in 0..cursor_height {
        let y = origin_y + cy as i32;
        if y < 0 || y >= height as i32 {
            continue;
        }
        for cx in 0..cursor_width {
            let x = origin_x + cx as i32;
            if x < 0 || x >= width as i32 {
                continue;
            }
            let src = cy * pitch + cx * 4;
            if src + 4 > cursor.len() {
                continue;
            }
            let alpha = cursor[src + 3];
            let dst = (y as usize * width + x as usize) * 4;
            if alpha == 0xff {
                for channel in 0..3 {
                    frame[dst + channel] ^= cursor[src + channel];
                }
            } else if alpha == 0 {
                frame[dst..dst + 3].copy_from_slice(&cursor[src..src + 3]);
                frame[dst + 3] = 255;
            } else {
                for channel in 0..3 {
                    let foreground = cursor[src + channel] as u32;
                    let background = frame[dst + channel] as u32;
                    frame[dst + channel] = ((foreground * alpha as u32
                        + background * (255 - alpha as u32))
                        / 255) as u8;
                }
                frame[dst + 3] = 255;
            }
        }
    }
}

fn overlay_monochrome_pointer_bgra(
    frame: &mut [u8],
    width: usize,
    height: usize,
    cursor: &[u8],
    pitch: usize,
    cursor_width: usize,
    cursor_height: usize,
    origin_x: i32,
    origin_y: i32,
) {
    let mask_bytes = pitch.saturating_mul(cursor_height);
    if cursor.len() < mask_bytes.saturating_mul(2) {
        return;
    }
    for cy in 0..cursor_height {
        let y = origin_y + cy as i32;
        if y < 0 || y >= height as i32 {
            continue;
        }
        for cx in 0..cursor_width {
            let x = origin_x + cx as i32;
            if x < 0 || x >= width as i32 {
                continue;
            }
            let byte_index = cy * pitch + cx / 8;
            let bit = 0x80 >> (cx % 8);
            let and_bit = (cursor[byte_index] & bit) != 0;
            let xor_bit = (cursor[mask_bytes + byte_index] & bit) != 0;
            let dst = (y as usize * width + x as usize) * 4;
            match (and_bit, xor_bit) {
                (false, false) => {
                    frame[dst] = 0;
                    frame[dst + 1] = 0;
                    frame[dst + 2] = 0;
                    frame[dst + 3] = 255;
                }
                (false, true) => {
                    frame[dst] = 255;
                    frame[dst + 1] = 255;
                    frame[dst + 2] = 255;
                    frame[dst + 3] = 255;
                }
                (true, false) => {}
                (true, true) => {
                    for channel in 0..3 {
                        frame[dst + channel] ^= 0xff;
                    }
                    frame[dst + 3] = 255;
                }
            }
        }
    }
}

fn render_system_cursor_shape(hicon: HICON) -> Option<PointerShape> {
    let width = unsafe { GetSystemMetrics(SM_CXCURSOR) }.max(1) as u32;
    let height = unsafe { GetSystemMetrics(SM_CYCURSOR) }.max(1) as u32;
    let mut icon_info = ICONINFO::default();
    if unsafe { GetIconInfo(hicon, &mut icon_info) }.is_err() {
        return None;
    }

    let result = (|| {
        let hdc = unsafe { CreateCompatibleDC(None) };
        if hdc.is_invalid() {
            return None;
        }

        let mut bits = std::ptr::null_mut();
        let mut bitmap_info = BITMAPINFO::default();
        bitmap_info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: width.saturating_mul(height).saturating_mul(4),
            ..Default::default()
        };
        let bitmap = unsafe {
            CreateDIBSection(
                hdc,
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut bits,
                HANDLE::default(),
                0,
            )
        }
        .ok()?;
        if bitmap.is_invalid() || bits.is_null() {
            unsafe {
                let _ = DeleteDC(hdc);
            }
            return None;
        }

        let old_object = unsafe { SelectObject(hdc, bitmap) };
        let draw_result = unsafe {
            DrawIconEx(
                hdc,
                0,
                0,
                hicon,
                width as i32,
                height as i32,
                0,
                None,
                DI_NORMAL,
            )
        };
        let len = width as usize * height as usize * 4;
        let bytes = if draw_result.is_ok() {
            let mut bytes = unsafe { std::slice::from_raw_parts(bits as *const u8, len) }.to_vec();
            if bytes.chunks_exact(4).all(|px| px[3] == 0) {
                for px in bytes.chunks_exact_mut(4) {
                    if px[0] != 0 || px[1] != 0 || px[2] != 0 {
                        px[3] = 255;
                    }
                }
            }
            Some(bytes)
        } else {
            None
        };
        if !old_object.is_invalid() {
            unsafe {
                let _ = SelectObject(hdc, old_object);
            }
        }
        unsafe {
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(hdc);
        }
        bytes
    })();

    unsafe {
        if !icon_info.hbmColor.is_invalid() {
            let _ = DeleteObject(icon_info.hbmColor);
        }
        if !icon_info.hbmMask.is_invalid() {
            let _ = DeleteObject(icon_info.hbmMask);
        }
    }

    let bytes = result?;
    Some(PointerShape {
        info: DXGI_OUTDUPL_POINTER_SHAPE_INFO {
            Type: DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32,
            Width: width,
            Height: height,
            Pitch: width.saturating_mul(4),
            HotSpot: POINT {
                x: icon_info.xHotspot as i32,
                y: icon_info.yHotspot as i32,
            },
        },
        bytes,
    })
}

fn clamp_u8(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

fn nv12_sample(
    nv12: &[u8],
    frame_index: u64,
    fps: u32,
    width: u32,
    height: u32,
) -> Result<IMFSample, String> {
    let expected_len = width as usize * height as usize * 3 / 2;
    if nv12.len() != expected_len {
        return Err(format!(
            "native_mf NV12 length mismatch: got {}, expected {}",
            nv12.len(),
            expected_len
        ));
    }
    let buffer = unsafe { MFCreateMemoryBuffer(nv12.len() as u32) }
        .map_err(|error| format!("MFCreateMemoryBuffer failed: {}", error))?;
    copy_to_media_buffer(&buffer, nv12)?;
    let sample =
        unsafe { MFCreateSample() }.map_err(|error| format!("MFCreateSample failed: {}", error))?;
    unsafe { sample.AddBuffer(&buffer) }.map_err(|error| format!("AddBuffer failed: {}", error))?;
    let duration = 10_000_000i64 / fps.max(1) as i64;
    unsafe { sample.SetSampleTime(frame_index.saturating_mul(duration as u64) as i64) }
        .map_err(|error| format!("SetSampleTime failed: {}", error))?;
    unsafe { sample.SetSampleDuration(duration) }
        .map_err(|error| format!("SetSampleDuration failed: {}", error))?;
    Ok(sample)
}

fn dxgi_texture_sample(
    texture: &ID3D11Texture2D,
    frame_index: u64,
    fps: u32,
) -> Result<IMFSample, String> {
    let buffer = unsafe { MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, texture, 0, false) }
        .map_err(|error| format!("MFCreateDXGISurfaceBuffer failed: {}", error))?;
    texture_sample_from_buffer(&buffer, frame_index, fps)
}

fn texture_sample_from_buffer(
    buffer: &IMFMediaBuffer,
    frame_index: u64,
    fps: u32,
) -> Result<IMFSample, String> {
    let sample =
        unsafe { MFCreateSample() }.map_err(|error| format!("MFCreateSample failed: {}", error))?;
    unsafe { sample.AddBuffer(buffer) }
        .map_err(|error| format!("AddBuffer(DXGI surface) failed: {}", error))?;
    let duration = 10_000_000i64 / fps.max(1) as i64;
    unsafe { sample.SetSampleTime(frame_index.saturating_mul(duration as u64) as i64) }
        .map_err(|error| format!("SetSampleTime failed: {}", error))?;
    unsafe { sample.SetSampleDuration(duration) }
        .map_err(|error| format!("SetSampleDuration failed: {}", error))?;
    Ok(sample)
}

fn output_sample(buffer_size: u32) -> Result<IMFSample, String> {
    let buffer = unsafe { MFCreateMemoryBuffer(buffer_size) }
        .map_err(|error| format!("MFCreateMemoryBuffer(output) failed: {}", error))?;
    let sample = unsafe { MFCreateSample() }
        .map_err(|error| format!("MFCreateSample(output) failed: {}", error))?;
    unsafe { sample.AddBuffer(&buffer) }
        .map_err(|error| format!("AddBuffer(output) failed: {}", error))?;
    Ok(sample)
}

fn copy_to_media_buffer(buffer: &IMFMediaBuffer, bytes: &[u8]) -> Result<(), String> {
    let mut dst = std::ptr::null_mut();
    let mut max_len = 0u32;
    unsafe { buffer.Lock(&mut dst, Some(&mut max_len), None) }
        .map_err(|error| format!("IMFMediaBuffer Lock failed: {}", error))?;
    let result = if max_len < bytes.len() as u32 {
        Err(format!(
            "IMFMediaBuffer too small: {} < {}",
            max_len,
            bytes.len()
        ))
    } else {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len()) };
        Ok(())
    };
    let unlock_result = unsafe { buffer.Unlock() }
        .map_err(|error| format!("IMFMediaBuffer Unlock failed: {}", error));
    result?;
    unlock_result?;
    unsafe { buffer.SetCurrentLength(bytes.len() as u32) }
        .map_err(|error| format!("SetCurrentLength failed: {}", error))?;
    Ok(())
}

fn sample_bytes(sample: &IMFSample) -> Result<Vec<u8>, String> {
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }
        .map_err(|error| format!("ConvertToContiguousBuffer failed: {}", error))?;
    let mut src = std::ptr::null_mut();
    let mut current_len = 0u32;
    unsafe { buffer.Lock(&mut src, None, Some(&mut current_len)) }
        .map_err(|error| format!("output IMFMediaBuffer Lock failed: {}", error))?;
    let bytes = if current_len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(src, current_len as usize).to_vec() }
    };
    unsafe { buffer.Unlock() }
        .map_err(|error| format!("output IMFMediaBuffer Unlock failed: {}", error))?;
    Ok(bytes)
}

fn media_type_blob(
    media_type: &IMFMediaType,
    key: &windows::core::GUID,
) -> Result<Vec<u8>, String> {
    let size = unsafe { media_type.GetBlobSize(key) }
        .map_err(|error| format!("GetBlobSize failed: {}", error))?;
    let mut bytes = vec![0u8; size as usize];
    let mut written = 0u32;
    unsafe { media_type.GetBlob(key, &mut bytes, Some(&mut written)) }
        .map_err(|error| format!("GetBlob failed: {}", error))?;
    bytes.truncate(written as usize);
    Ok(bytes)
}

fn has_start_code(bytes: &[u8]) -> bool {
    bytes.windows(3).any(|window| window == [0, 0, 1])
        || bytes.windows(4).any(|window| window == [0, 0, 0, 1])
}

fn length_prefixed_hevc_to_annex_b(bytes: &[u8], length_size: usize) -> Result<Vec<u8>, String> {
    let mut offset = 0usize;
    let mut out = Vec::with_capacity(bytes.len() + 16);
    while offset < bytes.len() {
        if offset + length_size > bytes.len() {
            return Err("length-prefixed HEVC sample ended inside NAL length".to_string());
        }
        let mut len = 0usize;
        for byte in &bytes[offset..offset + length_size] {
            len = (len << 8) | *byte as usize;
        }
        offset += length_size;
        if len == 0 {
            continue;
        }
        if offset + len > bytes.len() {
            return Err("length-prefixed HEVC sample NAL length exceeds sample size".to_string());
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&bytes[offset..offset + len]);
        offset += len;
    }
    Ok(out)
}

fn mft_friendly_name(
    activate: &windows::Win32::Media::MediaFoundation::IMFActivate,
) -> Option<String> {
    let mut value = PWSTR::null();
    let mut len = 0u32;
    let result =
        unsafe { activate.GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut value, &mut len) };
    if result.is_err() || value.is_null() {
        return None;
    }
    let name =
        unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(value.0, len as usize)) };
    unsafe { CoTaskMemFree(Some(value.0.cast())) };
    Some(name)
}
