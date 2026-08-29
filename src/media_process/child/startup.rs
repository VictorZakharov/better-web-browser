use crate::media_protocol::{
    MEDIA_HEADER_LENGTH, MEDIA_MAGIC, MEDIA_PROTOCOL_MINOR, MediaSessionId,
};
use std::fs::File;
use std::io::Write;

pub(super) fn write_raw_header(
    mut output: File,
    session: MediaSessionId,
    major: u16,
    payload_length: u32,
) -> Result<(), String> {
    let mut header = [0_u8; MEDIA_HEADER_LENGTH];
    header[..4].copy_from_slice(&MEDIA_MAGIC);
    header[4..6].copy_from_slice(&major.to_le_bytes());
    header[6..8].copy_from_slice(&MEDIA_PROTOCOL_MINOR.to_le_bytes());
    header[8..10].copy_from_slice(&2_u16.to_le_bytes());
    header[12..16].copy_from_slice(&payload_length.to_le_bytes());
    header[16..24].copy_from_slice(&session.get().to_le_bytes());
    header[24..32].copy_from_slice(&1_u64.to_le_bytes());
    output
        .write_all(&header)
        .map_err(|error| format!("write raw media test frame: {error}"))
}
