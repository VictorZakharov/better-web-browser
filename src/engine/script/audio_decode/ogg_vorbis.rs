//! Decode one complete Ogg/Vorbis logical stream into Web Audio float PCM.

use super::{DecodedAudio, MAX_DECODED_BYTES, MAX_ENCODED_BYTES, resample};
use crate::ogg_vorbis_headers;
use lewton::inside_ogg::OggStreamReader;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};

// RFC 3533 permits chaining and grouping, but this decoder has one AudioBuffer
// format and one allocation budget. Require contiguous pages for one stream.
fn validate_container(bytes: &[u8], cancelled: &AtomicBool) -> Result<(), String> {
    if !bytes.starts_with(b"OggS") || bytes.len() > MAX_ENCODED_BYTES {
        return Err("audio data is not a bounded Ogg stream".into());
    }
    let mut offset = 0usize;
    let mut serial = None;
    let mut sequence = None::<u32>;
    let mut ended = false;
    while offset < bytes.len() {
        if cancelled.load(Ordering::Relaxed) {
            return Err("audio decoding was cancelled".into());
        }
        if ended {
            return Err("chained or trailing Ogg data is not supported".into());
        }
        let header = bytes
            .get(offset..offset.saturating_add(27))
            .ok_or("Ogg page header is truncated")?;
        if &header[..4] != b"OggS" || header[4] != 0 || header[5] & !0x07 != 0 {
            return Err("Ogg page header is invalid".into());
        }
        let flags = header[5];
        let page_serial = u32::from_le_bytes(header[14..18].try_into().unwrap());
        let page_sequence = u32::from_le_bytes(header[18..22].try_into().unwrap());
        if serial.is_none() {
            if flags & 0x02 == 0 || flags & 0x01 != 0 {
                return Err("Ogg beginning-of-stream page is missing".into());
            }
            serial = Some(page_serial);
        } else if serial != Some(page_serial)
            || flags & 0x02 != 0
            || sequence.is_some_and(|previous| page_sequence != previous.wrapping_add(1))
        {
            return Err("Ogg stream serial or page sequence is invalid".into());
        }
        sequence = Some(page_sequence);
        let segment_count = usize::from(header[26]);
        let segments_start = offset + 27;
        let segments = bytes
            .get(segments_start..segments_start + segment_count)
            .ok_or("Ogg segment table is truncated")?;
        let payload_len: usize = segments.iter().map(|length| usize::from(*length)).sum();
        offset = (segments_start + segment_count)
            .checked_add(payload_len)
            .filter(|end| *end <= bytes.len())
            .ok_or("Ogg page payload is truncated")?;
        ended = flags & 0x04 != 0;
    }
    if !ended {
        return Err("Ogg end-of-stream page is missing".into());
    }
    Ok(())
}

pub(super) fn decode(
    bytes: &[u8],
    target_rate: f64,
    cancelled: &AtomicBool,
) -> Result<DecodedAudio, String> {
    validate_container(bytes, cancelled)?;
    let (input_rate, channel_count) = ogg_vorbis_headers::preflight(bytes, 32)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err("audio decoding was cancelled".into());
    }
    let mut reader = OggStreamReader::new(Cursor::new(bytes))
        .map_err(|error| format!("open Ogg/Vorbis decoder: {error}"))?;
    if reader.ident_hdr.audio_sample_rate != input_rate
        || u16::from(reader.ident_hdr.audio_channels) != channel_count
    {
        return Err("Ogg/Vorbis headers changed during decoder initialization".into());
    }
    let serial = reader.stream_serial();
    let channels = usize::from(channel_count);
    let mut source = vec![Vec::<f32>::new(); channels];
    let mut frames = 0usize;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("audio decoding was cancelled".into());
        }
        let Some(packet) = reader
            .read_dec_packet_generic::<Vec<Vec<f32>>>()
            .map_err(|error| format!("decode Ogg/Vorbis packet: {error}"))?
        else {
            break;
        };
        if reader.stream_serial() != serial || packet.len() != channels {
            return Err("Ogg/Vorbis channel or stream changed".into());
        }
        let packet_frames = packet.first().map_or(0, Vec::len);
        if packet.iter().any(|channel| channel.len() != packet_frames) {
            return Err("Ogg/Vorbis PCM channels have unequal lengths".into());
        }
        frames = frames
            .checked_add(packet_frames)
            .ok_or("decoded audio size overflow")?;
        let source_bytes = frames
            .checked_mul(channels)
            .and_then(|samples| samples.checked_mul(4))
            .ok_or("decoded audio size overflow")?;
        if source_bytes > MAX_DECODED_BYTES {
            return Err("decoded audio exceeds the 16 MiB AudioBuffer limit".into());
        }
        if frames > 0 {
            resample::output_frames(frames, channels, input_rate, target_rate)?;
        }
        for (destination, samples) in source.iter_mut().zip(packet) {
            destination.extend(
                samples
                    .into_iter()
                    .map(|sample| if sample.is_finite() { sample } else { 0.0 }),
            );
        }
    }
    if frames == 0 {
        return Err("Ogg/Vorbis source produced no PCM".into());
    }
    resample::finish(source, input_rate, target_rate, cancelled)
}

#[cfg(test)]
mod tests;
