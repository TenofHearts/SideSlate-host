#![allow(dead_code)]

use std::io::{Read, Write};

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

pub const CAP_HEVC: u32 = 0x0000_0001;
pub const CAP_STATS: u32 = 0x0000_0004;
pub const CAP_KEYFRAME_REQUEST: u32 = 0x0000_0008;

pub const CODEC_HEVC: u8 = 1;

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
    writer
        .write_all(&header)
        .map_err(|err| format!("socket header write failed: {}", err))?;
    writer
        .write_all(payload)
        .map_err(|err| format!("socket payload write failed: {}", err))?;
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
    payload.extend_from_slice(&(CAP_HEVC | CAP_STATS | CAP_KEYFRAME_REQUEST).to_le_bytes());
    payload
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
