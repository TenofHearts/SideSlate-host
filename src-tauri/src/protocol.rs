#![allow(dead_code)]

use std::io::{IoSlice, Read, Write};

pub const MAGIC: [u8; 4] = *b"T2S1";
pub const VERSION: u8 = 1;
pub const HEADER_SIZE: usize = 24;
pub const MAX_PAYLOAD_LEN: u32 = 64 * 1024 * 1024;

pub const TYPE_HELLO: u8 = 1;
pub const TYPE_HELLO_ACK: u8 = 2;
pub const TYPE_VIDEO_CONFIG: u8 = 3;
pub const TYPE_VIDEO_CONFIG_ACK: u8 = 4;
pub const TYPE_VIDEO_PACKET: u8 = 5;
pub const TYPE_KEYFRAME_REQUEST: u8 = 6;
pub const TYPE_STATS: u8 = 7;
pub const TYPE_ERROR: u8 = 8;
pub const TYPE_STOP: u8 = 9;

pub const FLAG_KEYFRAME: u16 = 0x0001;
pub const FLAG_CONFIG_NAL: u16 = 0x0002;
pub const FLAG_VCL: u16 = 0x0004;
pub const FLAG_DROPPABLE: u16 = 0x0020;
pub const FLAG_FRAGMENT: u16 = 0x0040;

pub const CAP_HEVC: u32 = 0x0000_0001;
pub const CAP_STATS: u32 = 0x0000_0004;
pub const CAP_KEYFRAME_REQUEST: u32 = 0x0000_0008;
pub const CAP_FRAGMENTED_VIDEO: u32 = 0x0000_0010;

pub const CODEC_HEVC: u8 = 1;
pub const VIDEO_FRAGMENT_HEADER_SIZE: usize = 20;

pub const RECEIVER_STATS_PAYLOAD_SIZE: usize = 152;
pub const RECEIVER_STATS_EXTENDED_PAYLOAD_SIZE: usize = 200;

#[derive(Debug, Clone)]
pub struct Message {
    pub message_type: u8,
    pub flags: u16,
    pub sequence: u32,
    pub timestamp_us: u64,
    pub payload: Vec<u8>,
}

pub fn write_message<W: Write>(writer: &mut W, message: &Message) -> Result<(), String> {
    write_message_parts(
        writer,
        message.message_type,
        message.flags,
        message.sequence,
        message.timestamp_us,
        &message.payload,
    )
}

pub fn write_message_parts<W: Write>(
    writer: &mut W,
    message_type: u8,
    flags: u16,
    sequence: u32,
    timestamp_us: u64,
    payload: &[u8],
) -> Result<(), String> {
    if payload.len() > MAX_PAYLOAD_LEN as usize {
        return Err(format!("payload too large: {}", payload.len()));
    }

    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&MAGIC);
    header[4] = VERSION;
    header[5] = message_type;
    header[6..8].copy_from_slice(&flags.to_le_bytes());
    header[8..12].copy_from_slice(&sequence.to_le_bytes());
    header[12..20].copy_from_slice(&timestamp_us.to_le_bytes());
    header[20..24].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    write_all_vectored(writer, &[&header, payload])
        .map_err(|err| format!("socket message write failed: {}", err))?;
    Ok(())
}

fn write_all_vectored<W: Write>(writer: &mut W, parts: &[&[u8]; 2]) -> std::io::Result<()> {
    let mut header_offset = 0usize;
    let mut payload_offset = 0usize;

    while header_offset < parts[0].len() || payload_offset < parts[1].len() {
        let written = if header_offset < parts[0].len() {
            writer.write_vectored(&[
                IoSlice::new(&parts[0][header_offset..]),
                IoSlice::new(&parts[1][payload_offset..]),
            ])?
        } else {
            writer.write(&parts[1][payload_offset..])?
        };
        if written == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "failed to write message",
            ));
        }

        let mut remaining = written;
        if header_offset < parts[0].len() {
            let header_remaining = parts[0].len() - header_offset;
            let consumed = remaining.min(header_remaining);
            header_offset += consumed;
            remaining -= consumed;
        }
        if remaining > 0 {
            payload_offset += remaining;
        }
    }

    Ok(())
}

pub fn message_header(
    message_type: u8,
    flags: u16,
    sequence: u32,
    timestamp_us: u64,
    payload_len: usize,
) -> Result<[u8; HEADER_SIZE], String> {
    if payload_len > MAX_PAYLOAD_LEN as usize {
        return Err(format!("payload too large: {}", payload_len));
    }

    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&MAGIC);
    header[4] = VERSION;
    header[5] = message_type;
    header[6..8].copy_from_slice(&flags.to_le_bytes());
    header[8..12].copy_from_slice(&sequence.to_le_bytes());
    header[12..20].copy_from_slice(&timestamp_us.to_le_bytes());
    header[20..24].copy_from_slice(&(payload_len as u32).to_le_bytes());
    Ok(header)
}

pub fn read_message<R: Read>(reader: &mut R) -> Result<Message, String> {
    let mut header = [0u8; HEADER_SIZE];
    reader
        .read_exact(&mut header)
        .map_err(|err| format!("socket header read failed: {}", err))?;

    if header[0..4] != MAGIC {
        return Err("bad protocol magic".to_string());
    }
    if header[4] != VERSION {
        return Err(format!("unsupported protocol version: {}", header[4]));
    }

    let payload_len = u32::from_le_bytes(header[20..24].try_into().unwrap());
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(format!("payload too large: {}", payload_len));
    }

    let mut payload = vec![0u8; payload_len as usize];
    reader
        .read_exact(&mut payload)
        .map_err(|err| format!("socket payload read failed: {}", err))?;

    Ok(Message {
        message_type: header[5],
        flags: u16::from_le_bytes(header[6..8].try_into().unwrap()),
        sequence: u32::from_le_bytes(header[8..12].try_into().unwrap()),
        timestamp_us: u64::from_le_bytes(header[12..20].try_into().unwrap()),
        payload,
    })
}

pub fn hello_payload() -> Vec<u8> {
    let mut payload = Vec::with_capacity(8);
    payload.extend_from_slice(&[1, VERSION, VERSION, 0]);
    payload.extend_from_slice(
        &(CAP_HEVC | CAP_STATS | CAP_KEYFRAME_REQUEST | CAP_FRAGMENTED_VIDEO).to_le_bytes(),
    );
    payload
}

pub fn video_fragment_payload(
    frame_sequence: u32,
    fragment_index: u16,
    fragment_count: u16,
    frame_flags: u16,
    fragment_offset: usize,
    total_len: usize,
    fragment: &[u8],
) -> Result<Vec<u8>, String> {
    if fragment_count == 0 || fragment_index >= fragment_count {
        return Err(format!(
            "invalid fragment index {}/{}",
            fragment_index, fragment_count
        ));
    }
    if fragment_offset > total_len || fragment_offset + fragment.len() > total_len {
        return Err(format!(
            "invalid fragment range offset={} len={} total={}",
            fragment_offset,
            fragment.len(),
            total_len
        ));
    }
    if total_len > MAX_PAYLOAD_LEN as usize || fragment_offset > u32::MAX as usize {
        return Err(format!("fragmented frame too large: {}", total_len));
    }
    let mut payload = Vec::with_capacity(VIDEO_FRAGMENT_HEADER_SIZE + fragment.len());
    payload.extend_from_slice(&frame_sequence.to_le_bytes());
    payload.extend_from_slice(&fragment_index.to_le_bytes());
    payload.extend_from_slice(&fragment_count.to_le_bytes());
    payload.extend_from_slice(&frame_flags.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.extend_from_slice(&(fragment_offset as u32).to_le_bytes());
    payload.extend_from_slice(&(total_len as u32).to_le_bytes());
    payload.extend_from_slice(fragment);
    Ok(payload)
}

pub fn video_config_payload(
    width: u16,
    height: u16,
    fps: u16,
    bitrate_kbps: u32,
    gop: u16,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(16);
    payload.push(CODEC_HEVC);
    payload.push(0);
    payload.extend_from_slice(&width.to_le_bytes());
    payload.extend_from_slice(&height.to_le_bytes());
    payload.extend_from_slice(&fps.to_le_bytes());
    payload.extend_from_slice(&1u16.to_le_bytes());
    payload.extend_from_slice(&bitrate_kbps.to_le_bytes());
    payload.extend_from_slice(&gop.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64_le(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_f64_le(data: &[u8], offset: usize) -> Option<f64> {
    Some(f64::from_le_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[derive(Debug, Default, Clone)]
pub struct ReceiverStats {
    pub running: bool,
    pub decoder_started: bool,
    pub surface_ready: bool,
    pub packets: u64,
    pub bytes: u64,
    pub queued_inputs: u64,
    pub rendered_outputs: u64,
    pub dropped_packets: u64,
    pub sequence_gaps: u64,
    pub config_packets: u64,
    pub keyframes: u64,
    pub last_sequence: u32,
    pub queue_depth: u32,
    pub stream_width: i32,
    pub stream_height: i32,
    pub stream_fps: i32,
    pub last_error: i32,
    pub receive_mbps: f64,
    pub input_fps: f64,
    pub render_fps: f64,
    pub drop_fps: f64,
    pub max_receive_gap_ms: f64,
    pub max_input_gap_ms: f64,
    pub max_render_gap_ms: f64,
    pub latest_receive_to_input_ms: f64,
    pub latest_input_to_render_ms: f64,
    pub latest_receive_to_render_ms: f64,
    pub max_receive_to_input_ms: f64,
    pub max_input_to_render_ms: f64,
    pub max_receive_to_render_ms: f64,
}

pub fn parse_receiver_stats_payload(payload: &[u8]) -> Option<ReceiverStats> {
    if payload.len() < RECEIVER_STATS_PAYLOAD_SIZE {
        return None;
    }
    let flags = read_u32_le(payload, 0)?;
    Some(ReceiverStats {
        running: flags & 0x01 != 0,
        decoder_started: flags & 0x02 != 0,
        surface_ready: flags & 0x04 != 0,
        packets: read_u64_le(payload, 8)?,
        bytes: read_u64_le(payload, 16)?,
        queued_inputs: read_u64_le(payload, 24)?,
        rendered_outputs: read_u64_le(payload, 32)?,
        dropped_packets: read_u64_le(payload, 40)?,
        sequence_gaps: read_u64_le(payload, 48)?,
        config_packets: read_u64_le(payload, 56)?,
        keyframes: read_u64_le(payload, 64)?,
        last_sequence: read_u32_le(payload, 72)?,
        queue_depth: read_u32_le(payload, 76)?,
        stream_width: read_i32_le(payload, 80)?,
        stream_height: read_i32_le(payload, 84)?,
        stream_fps: read_i32_le(payload, 88)?,
        last_error: read_i32_le(payload, 92)?,
        receive_mbps: read_f64_le(payload, 96)?,
        input_fps: read_f64_le(payload, 104)?,
        render_fps: read_f64_le(payload, 112)?,
        drop_fps: read_f64_le(payload, 120)?,
        max_receive_gap_ms: read_f64_le(payload, 128)?,
        max_input_gap_ms: read_f64_le(payload, 136)?,
        max_render_gap_ms: read_f64_le(payload, 144)?,
        latest_receive_to_input_ms: read_f64_le(payload, 152).unwrap_or(0.0),
        latest_input_to_render_ms: read_f64_le(payload, 160).unwrap_or(0.0),
        latest_receive_to_render_ms: read_f64_le(payload, 168).unwrap_or(0.0),
        max_receive_to_input_ms: read_f64_le(payload, 176).unwrap_or(0.0),
        max_input_to_render_ms: read_f64_le(payload, 184).unwrap_or(0.0),
        max_receive_to_render_ms: read_f64_le(payload, 192).unwrap_or(0.0),
    })
}

pub fn expect_type(message: Message, expected: u8) -> Result<Message, String> {
    if message.message_type == TYPE_ERROR {
        let error = String::from_utf8_lossy(&message.payload);
        return Err(format!("receiver error: {}", error));
    }
    if message.message_type != expected {
        return Err(format!(
            "unexpected message type {}, expected {}",
            message.message_type, expected
        ));
    }
    Ok(message)
}
