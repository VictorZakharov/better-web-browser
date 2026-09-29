//! Bounded Vorbis header preflight shared by media playback and Web Audio.

use ogg::reading::PacketReader;
use std::io::Cursor;

mod setup_guard;

const MAX_HEADER_PACKET_BYTES: usize = 256 * 1024;
const MIN_SAMPLE_RATE: u32 = 8_000;
const MAX_SAMPLE_RATE: u32 = 192_000;

pub(crate) fn preflight(bytes: &[u8], max_channels: u16) -> Result<(u32, u16), String> {
    if !bytes.starts_with(b"OggS") {
        return Err("source is not an Ogg stream".into());
    }
    let mut packets = PacketReader::new(Cursor::new(bytes));
    let mut serial = None;
    let mut format = None;
    for (index, kind) in [1_u8, 3, 5].into_iter().enumerate() {
        let packet = packets
            .read_packet()
            .map_err(|error| format!("read Ogg/Vorbis header: {error}"))?
            .ok_or_else(|| "Ogg/Vorbis headers are incomplete".to_string())?;
        if packet.data.len() > MAX_HEADER_PACKET_BYTES
            || packet.data.get(..7) != Some(&[kind, b'v', b'o', b'r', b'b', b'i', b's'])
            || serial.is_some_and(|value| value != packet.stream_serial())
            || (index == 0 && !packet.first_in_stream())
        {
            return Err("Ogg/Vorbis header is invalid or too large".into());
        }
        serial = Some(packet.stream_serial());
        match index {
            0 => {
                let header = lewton::header::read_header_ident(&packet.data)
                    .map_err(|error| format!("read Vorbis identification: {error}"))?;
                let channels = u16::from(header.audio_channels);
                if !(1..=max_channels).contains(&channels)
                    || !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&header.audio_sample_rate)
                {
                    return Err("Ogg/Vorbis format exceeds supported PCM output".into());
                }
                format = Some((header.audio_sample_rate, channels));
            }
            1 => validate_comments(&packet.data)?,
            _ => setup_guard::inspect(&packet.data)?,
        }
    }
    format.ok_or_else(|| "Ogg/Vorbis identification is missing".into())
}

fn validate_comments(packet: &[u8]) -> Result<(), String> {
    // Lewton allocates from the vendor/comment lengths before it checks EOF.
    // Reject impossible sizes first, using the actual bounded packet length.
    let mut offset = 7;
    let vendor = take_length(packet, &mut offset)?;
    offset = offset
        .checked_add(vendor)
        .filter(|end| *end <= packet.len())
        .ok_or_else(|| "Ogg/Vorbis vendor length exceeds header".to_string())?;
    let count = take_length(packet, &mut offset)?;
    if count > packet.len() / 4 {
        return Err("Ogg/Vorbis comment count exceeds header".into());
    }
    for _ in 0..count {
        let length = take_length(packet, &mut offset)?;
        offset = offset
            .checked_add(length)
            .filter(|end| *end <= packet.len())
            .ok_or_else(|| "Ogg/Vorbis comment length exceeds header".to_string())?;
    }
    if packet.get(offset) != Some(&1) {
        return Err("Ogg/Vorbis comment framing is invalid".into());
    }
    Ok(())
}

fn take_length(packet: &[u8], offset: &mut usize) -> Result<usize, String> {
    let bytes: [u8; 4] = packet
        .get(*offset..offset.saturating_add(4))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| "Ogg/Vorbis comment header is truncated".to_string())?;
    *offset += 4;
    Ok(u32::from_le_bytes(bytes) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_hostile_comment_and_setup_lengths_before_decoder_allocations() {
        let mut comment = b"\x03vorbis".to_vec();
        comment.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(validate_comments(&comment).is_err());
        let mut comment = b"\x03vorbis".to_vec();
        comment.extend_from_slice(&0_u32.to_le_bytes());
        comment.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(validate_comments(&comment).is_err());

        let mut setup = b"\x05vorbis".to_vec();
        setup.extend_from_slice(&[0, 0x42, 0x43, 0x56, 1, 0, 0xff, 0xff, 0xff]);
        assert!(setup_guard::inspect(&setup).is_err());
    }
}
