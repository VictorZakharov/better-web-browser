//! FLAC registration metadata and upstream packet-integrity preflight.
#[cfg(test)]
mod tests;
use std::io::Cursor;
use symphonia::core::checksum::{Crc8Ccitt, Crc16Ansi};
use symphonia::core::codecs::audio::{AudioCodecParameters, well_known::CODEC_ID_FLAC};
use symphonia::core::io::{BufReader, Monitor};
use symphonia_common::xiph::audio::flac::StreamInfo;

pub(super) fn parameters(description: Option<&[u8]>) -> Result<AudioCodecParameters, String> {
    let bytes = description.ok_or("FLAC description must include fLaC and STREAMINFO")?;
    if bytes.len() > 65_536 || bytes.get(..4) != Some(b"fLaC") {
        return Err("FLAC description lacks its marker or exceeds 64 KiB".into());
    }
    let mut offset = 4;
    let mut blocks = 0;
    loop {
        let header = bytes
            .get(offset..offset + 4)
            .ok_or("FLAC metadata header is truncated")?;
        let kind = header[0] & 127;
        let size =
            (usize::from(header[1]) << 16) | (usize::from(header[2]) << 8) | usize::from(header[3]);
        if kind == 127 || (blocks == 0 && (kind != 0 || size != 34)) || (blocks != 0 && kind == 0) {
            return Err("FLAC requires a single initial 34-byte STREAMINFO block".into());
        }
        offset = offset
            .checked_add(4 + size)
            .filter(|end| *end <= bytes.len())
            .ok_or("FLAC metadata payload is truncated")?;
        blocks += 1;
        if blocks > 256 {
            return Err("FLAC description exceeds 256 metadata blocks".into());
        }
        if header[0] & 128 != 0 {
            break;
        }
    }
    if offset != bytes.len() {
        return Err("FLAC description contains trailing frame data".into());
    }
    let info = StreamInfo::read(&mut BufReader::new(&bytes[8..42]))
        .map_err(|error| format!("invalid FLAC STREAMINFO: {error}"))?;
    // The current decoder's stereo decorrelation uses i32 arithmetic. Keep a
    // headroom bit instead of claiming unsafe 32-bit bitstreams are supported.
    if info.sample_rate > 384_000 || !matches!(info.bits_per_sample, 8 | 12 | 16 | 20 | 24) {
        return Err(
            "FLAC packet decoder supports rates up to 384 kHz and 8/12/16/20/24-bit PCM".into(),
        );
    }
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_FLAC)
        .with_extra_data(bytes[8..42].to_vec().into_boxed_slice());
    Ok(params)
}

#[cfg(test)]
pub(super) fn validate_packet(description: &[u8], bytes: &[u8]) -> Result<(), String> {
    decode(description, bytes, 0).map(|_| ())
}

pub(super) fn decode(
    description: &[u8],
    bytes: &[u8],
    timestamp: i64,
) -> Result<Vec<super::Output>, String> {
    let header_end = validate_dimensions(description, bytes)?;
    if bytes
        .get(..2)
        .is_none_or(|head| head[0] != 255 || head[1] & 0xfe != 0xf8)
    {
        return Err("FLAC chunk must begin with one elementary FRAME".into());
    }
    let info = StreamInfo::read(&mut BufReader::new(&description[8..42]))
        .map_err(|error| format!("invalid FLAC metadata: {error}"))?;
    let normalized = normalize_depth(bytes, header_end, info.bits_per_sample)?;
    // Claxon's frame reader consumes one syntactic frame and verifies both
    // CRCs. An unbuffered Cursor proves exact consumption, including CRC-valid
    // zero suffixes that a resynchronizing container parser could absorb.
    let mut reader = claxon::frame::FrameReader::new(Cursor::new(normalized));
    let block = reader
        .read_next_or_eof(Vec::new())
        .map_err(|error| format!("decode elementary FLAC frame: {error}"))?
        .ok_or("FLAC packet has no complete frame")?;
    if reader.into_inner().position() != bytes.len() as u64 {
        return Err("FLAC chunk must contain exactly one frame without trailing bytes".into());
    }
    let frames = block.duration();
    let channels = block.channels();
    if channels as usize != info.channels.count() || frames > u32::from(info.block_len_max) {
        return Err("FLAC decoder output disagrees with admitted dimensions".into());
    }
    let mut pcm = Vec::with_capacity(frames as usize * channels as usize * 4);
    for channel in 0..channels {
        for sample in 0..frames {
            pcm.extend_from_slice(
                &block
                    .sample(channel, sample)
                    .wrapping_shl(32 - info.bits_per_sample)
                    .to_le_bytes(),
            );
        }
    }
    Ok(vec![super::Output {
        bytes: pcm,
        format: "s32-planar",
        timestamp,
        frames,
        duration: u64::from(frames) * 1_000_000 / u64::from(info.sample_rate),
        sample_rate: info.sample_rate,
        channels,
        description: None,
    }])
}

fn validate_dimensions(description: &[u8], bytes: &[u8]) -> Result<usize, String> {
    let info = StreamInfo::read(&mut BufReader::new(&description[8..42]))
        .map_err(|error| format!("invalid FLAC metadata: {error}"))?;
    let head = bytes.get(..4).ok_or("FLAC frame header is truncated")?;
    let assignment = head[3] >> 4;
    let channels = match assignment {
        0..=7 => u32::from(assignment) + 1,
        8..=10 => 2,
        _ => return Err("FLAC frame channel assignment is reserved".into()),
    };
    let bits = match (head[3] >> 1) & 7 {
        0 => info.bits_per_sample,
        1 => 8,
        2 => 12,
        4 => 16,
        5 => 20,
        6 => 24,
        _ => return Err("FLAC frame bit depth is unsupported or reserved".into()),
    };
    if channels as usize != info.channels.count() || bits != info.bits_per_sample {
        return Err("FLAC frame dimensions disagree with STREAMINFO".into());
    }
    // Only inspect dimension fields here. The upstream parser owns canonical
    // UTF-8 sample numbering, header/frame CRCs and complete frame extraction.
    let first = *bytes
        .get(4)
        .ok_or("FLAC frame sample number is truncated")?;
    let length = if first < 128 {
        1
    } else {
        first.leading_ones() as usize
    };
    if !(1..=7).contains(&length) || length == 1 && first >= 128 {
        return Err("FLAC frame sample number is malformed".into());
    }
    let mut offset = 4 + length;
    let block = match head[2] >> 4 {
        1 => 192,
        2..=5 => 576_u32 << ((head[2] >> 4) - 2),
        6 => {
            let value = *bytes.get(offset).ok_or("FLAC block size is truncated")?;
            offset += 1;
            u32::from(value) + 1
        }
        7 => {
            let value: [u8; 2] = bytes
                .get(offset..offset + 2)
                .and_then(|b| b.try_into().ok())
                .ok_or("FLAC block size is truncated")?;
            offset += 2;
            u32::from(u16::from_be_bytes(value)) + 1
        }
        code @ 8..=15 => 256_u32 << (code - 8),
        _ => return Err("FLAC frame block size is reserved".into()),
    };
    if block > u32::from(info.block_len_max) {
        return Err("FLAC frame exceeds its STREAMINFO allocation bound".into());
    }
    let rate = match head[2] & 15 {
        0 => info.sample_rate,
        1 => 88_200,
        2 => 176_400,
        3 => 192_000,
        4 => 8_000,
        5 => 16_000,
        6 => 22_050,
        7 => 24_000,
        8 => 32_000,
        9 => 44_100,
        10 => 48_000,
        11 => 96_000,
        12 => {
            let value =
                u32::from(*bytes.get(offset).ok_or("FLAC sample rate is truncated")?) * 1000;
            offset += 1;
            value
        }
        code @ (13 | 14) => {
            let value: [u8; 2] = bytes
                .get(offset..offset + 2)
                .and_then(|b| b.try_into().ok())
                .ok_or("FLAC sample rate is truncated")?;
            offset += 2;
            u32::from(u16::from_be_bytes(value)) * if code == 14 { 10 } else { 1 }
        }
        _ => return Err("FLAC frame sample rate is reserved".into()),
    };
    if rate != info.sample_rate {
        return Err("FLAC frame sample rate disagrees with STREAMINFO".into());
    }
    if bytes.len() < offset + 3 {
        return Err("FLAC frame header/footer is truncated".into());
    }
    Ok(offset)
}

fn normalize_depth(
    bytes: &[u8],
    header_end: usize,
    bits: u32,
) -> Result<std::borrow::Cow<'_, [u8]>, String> {
    if bytes[3] & 14 != 0 {
        return Ok(std::borrow::Cow::Borrowed(bytes));
    }
    let mut header_crc = Crc8Ccitt::new(0);
    header_crc.process_buf_bytes(&bytes[..header_end]);
    let end = bytes.len() - 2;
    let mut frame_crc = Crc16Ansi::new(0);
    frame_crc.process_buf_bytes(&bytes[..end]);
    if header_crc.crc() != bytes[header_end]
        || frame_crc.crc() != u16::from_be_bytes(bytes[end..].try_into().unwrap())
    {
        return Err("FLAC original frame/header CRC mismatch".into());
    }
    // Claxon does not implement the frame-header STREAMINFO depth fallback.
    // Expand only this already-verified field in a private copy; the payload
    // is unchanged. Existing checksum implementations update its two CRCs.
    let code = match bits {
        8 => 1,
        12 => 2,
        16 => 4,
        20 => 5,
        24 => 6,
        _ => return Err("FLAC bit depth is unsupported".into()),
    };
    let mut normalized = bytes.to_vec();
    normalized[3] |= code << 1;
    let mut header_crc = Crc8Ccitt::new(0);
    header_crc.process_buf_bytes(&normalized[..header_end]);
    normalized[header_end] = header_crc.crc();
    let mut frame_crc = Crc16Ansi::new(0);
    frame_crc.process_buf_bytes(&normalized[..end]);
    normalized[end..].copy_from_slice(&frame_crc.crc().to_be_bytes());
    Ok(std::borrow::Cow::Owned(normalized))
}
