//! Complete logical-stream admission for the RFC 9639 FLAC-in-Ogg mapping.

use super::Budget;
use ogg::reading::{PacketReader, PageParser};
use std::io::Cursor;

const MAX_PACKET: usize = 256 * 1024;
mod metadata;

pub(super) fn sniff(bytes: &[u8]) -> bool {
    let Some(&segments) = bytes.get(26) else {
        return false;
    };
    let offset = 27 + usize::from(segments);
    bytes.get(offset..offset.saturating_add(5)) == Some(b"\x7fFLAC")
}

pub(super) fn validate(bytes: &[u8], budget: &mut Budget<'_>) -> Result<(), String> {
    let (serial, eos_granule) = pages(bytes, budget)?;
    let mut packets = PacketReader::new(Cursor::new(bytes));
    let first = packets
        .read_packet()
        .map_err(|error| format!("read Ogg FLAC mapping: {error}"))?
        .ok_or("Ogg FLAC mapping is missing")?;
    let data = &first.data;
    if !first.first_in_stream()
        || first.last_in_stream()
        || data.len() != 51
        || data.get(..7) != Some(b"\x7fFLAC\x01\x00")
        || data.get(9..13) != Some(b"fLaC")
        || data.get(14..17) != Some(&[0, 0, 34])
        || data[13] != 0
        || first.absgp_page() != 0
        || first.stream_serial() != serial
    {
        return Err("Ogg FLAC mapping or STREAMINFO is invalid".into());
    }
    stream_info(&data[17..], eos_granule)?;
    let declared_headers = u16::from_be_bytes([data[7], data[8]]) as usize;
    let mut headers = 0_usize;
    let mut comments = false;
    loop {
        budget.step()?;
        let packet = packets
            .read_packet()
            .map_err(|error| format!("read Ogg FLAC metadata: {error}"))?
            .ok_or("Ogg FLAC metadata is incomplete")?;
        let data = &packet.data;
        if packet.stream_serial() != serial
            || packet.first_in_stream()
            || packet.last_in_stream()
            || packet.absgp_page() != 0
            || data.len() < 4
        {
            return Err("Ogg FLAC metadata is incomplete or multiplexed".into());
        }
        let kind = data[0] & 0x7f;
        let length =
            (usize::from(data[1]) << 16) | (usize::from(data[2]) << 8) | usize::from(data[3]);
        if length != data.len() - 4 || kind == 0 || kind > 6 || (kind == 4 && comments) {
            return Err("Ogg FLAC metadata block type or length is invalid".into());
        }
        metadata::validate(kind, &data[4..])?;
        comments |= kind == 4;
        headers += 1;
        if headers > 256 {
            return Err("Ogg FLAC has too many metadata packets".into());
        }
        if data[0] & 0x80 != 0 {
            if !packet.last_in_page() {
                return Err("Ogg FLAC audio must begin after the final metadata page".into());
            }
            break;
        }
    }
    if declared_headers != 0 && declared_headers != headers {
        return Err("Ogg FLAC mapping header count does not match metadata".into());
    }
    Ok(())
}

fn stream_info(info: &[u8], eos: u64) -> Result<(), String> {
    let minimum = u16::from_be_bytes([info[0], info[1]]);
    let maximum = u16::from_be_bytes([info[2], info[3]]);
    let packed = u64::from_be_bytes(
        info[10..18]
            .try_into()
            .map_err(|_| "FLAC STREAMINFO is truncated")?,
    );
    let rate = packed >> 44;
    let bits = ((packed >> 36) & 31) + 1;
    let frames = packed & 0x0f_ffff_ffff;
    if minimum < 16
        || maximum < minimum
        || !(8_000..=192_000).contains(&rate)
        || !(4..=32).contains(&bits)
        || eos == 0
        || eos == u64::MAX
        || (frames != 0 && frames != eos)
    {
        return Err(
            "Ogg FLAC STREAMINFO or EOS sample count is unsupported or inconsistent".into(),
        );
    }
    Ok(())
}

fn pages(bytes: &[u8], budget: &mut Budget<'_>) -> Result<(u32, u64), String> {
    let mut remaining = bytes;
    let mut serial = None;
    let mut sequence = 0_u32;
    let mut continued = false;
    let mut packet_bytes = 0_usize;
    let mut ended = false;
    let mut eos = 0;
    while !remaining.is_empty() {
        budget.step()?;
        let header: [u8; 27] = remaining
            .get(..27)
            .and_then(|head| head.try_into().ok())
            .ok_or("Ogg page header is truncated")?;
        let current_serial = u32::from_le_bytes(header[14..18].try_into().unwrap());
        let current_sequence = u32::from_le_bytes(header[18..22].try_into().unwrap());
        let flags = header[5];
        if header[..4] != *b"OggS"
            || flags & !7 != 0
            || ended
            || current_sequence != sequence
            || serial.is_some_and(|value| value != current_serial)
            || (flags & 1 != 0) != continued
            || (flags & 2 != 0) != serial.is_none()
        {
            return Err(
                "Ogg FLAC requires consecutive pages of one complete logical stream".into(),
            );
        }
        serial = Some(current_serial);
        sequence = sequence
            .checked_add(1)
            .ok_or("Ogg page sequence overflows admission limit")?;
        let (mut parser, segments) =
            PageParser::new(header).map_err(|error| format!("read Ogg page: {error}"))?;
        let laces = remaining
            .get(27..27 + segments)
            .ok_or("Ogg page segment table is truncated")?;
        if sequence == 1 && laces != [51] {
            return Err("Ogg FLAC mapping must occupy its own 51-byte BOS packet/page".into());
        }
        for lace in laces {
            packet_bytes += usize::from(*lace);
            if packet_bytes > MAX_PACKET {
                return Err("Ogg packet exceeds admission allocation limit".into());
            }
            continued = *lace == 255;
            if !continued {
                packet_bytes = 0;
            }
        }
        let body_bytes = parser.parse_segments(laces.to_vec());
        let end = 27 + segments + body_bytes;
        let body = remaining
            .get(27 + segments..end)
            .ok_or("Ogg page body is truncated")?;
        parser
            .parse_packet_data(body.to_vec())
            .map_err(|error| format!("verify Ogg page: {error}"))?;
        ended = flags & 4 != 0;
        if ended {
            if continued {
                return Err("Ogg EOS leaves an incomplete packet".into());
            }
            eos = u64::from_le_bytes(header[6..14].try_into().unwrap());
        }
        remaining = &remaining[end..];
    }
    if !ended || sequence < 3 {
        return Err("Ogg FLAC source has no complete EOS audio page".into());
    }
    Ok((serial.ok_or("Ogg FLAC source has no pages")?, eos))
}

#[cfg(test)]
#[path = "ogg_flac_tests.rs"]
mod tests;
