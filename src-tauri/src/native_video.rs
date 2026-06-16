use serde::Serialize;
use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1, IDXGIResource,
    DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_DESC, DXGI_OUTDUPL_FRAME_INFO,
};
use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, IMFTransform, MFCreateMediaType, MFMediaType_Video, MFShutdown, MFStartup,
    MFTEnumEx, MFT_FRIENDLY_NAME_Attribute, MFVideoFormat_HEVC, MFVideoFormat_NV12,
    MFVideoInterlace_Progressive, MFSTARTUP_FULL, MFT_CATEGORY_VIDEO_ENCODER,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_REGISTER_TYPE_INFO, MF_LOW_LATENCY, MF_MT_AVG_BITRATE,
    MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_VERSION,
};
use windows::Win32::System::Com::CoTaskMemFree;

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
    let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
    let mut device = None;
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
            None,
        )
    }
    .map_err(|error| format!("D3D11CreateDevice failed: {}", error))?;
    device.ok_or_else(|| "D3D11CreateDevice returned no device".to_string())
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
                        configure_hevc_encoder(&transform, width, height, fps, bitrate).is_ok();
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

fn configure_hevc_encoder(
    transform: &IMFTransform,
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
) -> Result<(), String> {
    if let Ok(attributes) = unsafe { transform.GetAttributes() } {
        let _ = unsafe { attributes.SetUINT32(&MF_LOW_LATENCY, 1) };
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
