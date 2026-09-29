use super::{DecodedAudio, resample};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy)]
struct Format {
    encoding: u16,
    channels: usize,
    rate: u32,
    bits: u16,
    block_align: usize,
}

fn u16_at(bytes: &[u8], index: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(index..index + 2)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], index: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(index..index + 4)?.try_into().ok()?,
    ))
}

fn parse_format(bytes: &[u8]) -> Result<Format, String> {
    if bytes.len() < 16 {
        return Err("WAV format chunk is truncated".into());
    }
    let mut encoding = u16_at(bytes, 0).unwrap();
    let channels = usize::from(u16_at(bytes, 2).unwrap());
    let rate = u32_at(bytes, 4).unwrap();
    let byte_rate = u32_at(bytes, 8).unwrap();
    let block_align = usize::from(u16_at(bytes, 12).unwrap());
    let bits = u16_at(bytes, 14).unwrap();
    // WAVE_FORMAT_EXTENSIBLE carries the underlying PCM/float tag in its
    // standard GUID. Do not mistake another subtype for linear PCM.
    if encoding == 0xfffe {
        let suffix = [
            0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
        ];
        if bytes.len() < 40
            || u16_at(bytes, 16).unwrap_or(0) < 22
            || bytes.get(28..40) != Some(suffix.as_slice())
        {
            return Err("unsupported WAV extensible format".into());
        }
        let subtype = u32_at(bytes, 24).unwrap();
        encoding = u16::try_from(subtype).map_err(|_| "unsupported WAV subtype")?;
    }
    let sample_bytes = usize::from(bits / 8);
    if !(1..=32).contains(&channels)
        || !(8_000..=192_000).contains(&rate)
        || !matches!((encoding, bits), (1, 8 | 16 | 24 | 32) | (3, 32))
        || block_align != channels.checked_mul(sample_bytes).unwrap_or(0)
        || u64::from(byte_rate) != u64::from(rate) * block_align as u64
    {
        return Err("unsupported or invalid WAV PCM format".into());
    }
    Ok(Format {
        encoding,
        channels,
        rate,
        bits,
        block_align,
    })
}

fn parse(bytes: &[u8]) -> Result<(Format, &[u8]), String> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("audio data is not a supported RIFF/WAVE file".into());
    }
    let riff_len = usize::try_from(u32_at(bytes, 4).unwrap()).unwrap_or(usize::MAX);
    let end = riff_len.checked_add(8).ok_or("invalid RIFF length")?;
    if end < 12 || end > bytes.len() {
        return Err("RIFF/WAVE file is truncated".into());
    }
    let mut format = None;
    let mut data = None;
    let mut cursor = 12usize;
    while cursor + 8 <= end {
        let size = usize::try_from(u32_at(bytes, cursor + 4).unwrap())
            .map_err(|_| "WAV chunk is too large")?;
        let content = cursor + 8;
        let next = content
            .checked_add(size)
            .ok_or("invalid WAV chunk length")?;
        if next > end {
            return Err("WAV chunk is truncated".into());
        }
        match &bytes[cursor..cursor + 4] {
            b"fmt " if format.is_none() => format = Some(parse_format(&bytes[content..next])?),
            b"data" if data.is_none() => data = Some(&bytes[content..next]),
            _ => {}
        }
        cursor = next.checked_add(size & 1).ok_or("invalid WAV padding")?;
    }
    let format = format.ok_or("WAV format chunk is missing")?;
    let data = data.ok_or("WAV data chunk is missing")?;
    if data.is_empty() || !data.len().is_multiple_of(format.block_align) {
        return Err("WAV data has an incomplete sample frame".into());
    }
    Ok((format, data))
}

fn sample(bytes: &[u8], format: Format) -> f32 {
    match (format.encoding, format.bits) {
        (1, 8) => (f32::from(bytes[0]) - 128.0) / 128.0,
        (1, 16) => f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0,
        (1, 24) => {
            let value = i32::from_le_bytes([
                bytes[0],
                bytes[1],
                bytes[2],
                if bytes[2] & 0x80 != 0 { 0xff } else { 0 },
            ]);
            value as f32 / 8_388_608.0
        }
        (1, 32) => i32::from_le_bytes(bytes[..4].try_into().unwrap()) as f32 / 2_147_483_648.0,
        (3, 32) => {
            let value = f32::from_le_bytes(bytes[..4].try_into().unwrap());
            if value.is_finite() { value } else { 0.0 }
        }
        _ => unreachable!("format validated before sample conversion"),
    }
}

pub(super) fn decode(
    bytes: &[u8],
    target_rate: f64,
    cancelled: &AtomicBool,
) -> Result<DecodedAudio, String> {
    let (format, data) = parse(bytes)?;
    let input_frames = data.len() / format.block_align;
    resample::output_frames(input_frames, format.channels, format.rate, target_rate)?;
    let sample_bytes = usize::from(format.bits / 8);
    let mut source = vec![Vec::with_capacity(input_frames); format.channels];
    for (frame_index, frame) in data.chunks_exact(format.block_align).enumerate() {
        if frame_index.is_multiple_of(4096) && cancelled.load(Ordering::Relaxed) {
            return Err("audio decoding was cancelled".into());
        }
        for (channel, samples) in source.iter_mut().enumerate() {
            let start = channel * sample_bytes;
            samples.push(sample(&frame[start..start + sample_bytes], format));
        }
    }
    resample::finish(source, format.rate, target_rate, cancelled)
}
