//! Complete compressed audio files decoded on Web Audio's document-owned worker.
//! Symphonia probes and decodes inside the existing document-owned worker.

use super::{DecodedAudio, MAX_DECODED_BYTES, MAX_ENCODED_BYTES, resample};
use crate::encoded_audio::Kind;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::common::Limit;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

mod parameters;

fn mp3_has_declared_length(bytes: &[u8]) -> bool {
    // Symphonia also estimates a length for untagged MP3s from the first few
    // frame sizes. Only use a Xing/Info/VBRI count as evidence of truncation.
    let start = if bytes.starts_with(b"ID3") && bytes.len() >= 10 {
        let size = bytes[6..10].iter().fold(0usize, |value, byte| {
            (value << 7) | usize::from(byte & 0x7f)
        });
        10 + size + if bytes[5] & 0x10 != 0 { 10 } else { 0 }
    } else {
        0
    };
    let end = start.saturating_add(2_048).min(bytes.len());
    bytes.get(start..end).is_some_and(|prefix| {
        prefix
            .windows(4)
            .any(|marker| marker == b"Xing" || marker == b"Info" || marker == b"VBRI")
    })
}

pub(super) fn decode(
    bytes: &[u8],
    target_rate: f64,
    cancelled: &AtomicBool,
    kind: Kind,
) -> Result<DecodedAudio, String> {
    if bytes.is_empty() || bytes.len() > MAX_ENCODED_BYTES {
        return Err("encoded audio exceeds the 8 MiB decode limit".into());
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("audio decoding was cancelled".into());
    }
    if !target_rate.is_finite() || !(8_000.0..=192_000.0).contains(&target_rate) {
        return Err("the context sample rate is unsupported".into());
    }
    crate::encoded_audio::validate_with_cancel(bytes, kind, cancelled)?;
    let extension = kind.extension();
    // Symphonia currently returns decoded AAC packets rather than applying
    // the track's edit list. Preserve the container's presentation window so
    // encoder priming and final packet padding do not enter the AudioBuffer.
    let presentation_edit = match kind {
        Kind::AacM4a => crate::iso_bmff_audio::ordinary_audio_edit(bytes)?,
        _ => None,
    };
    let mut hint = Hint::new();
    hint.with_extension(extension);
    let media = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let metadata = MetadataOptions::default()
        .limit_tag_bytes(Limit::Maximum(64 * 1024))
        .limit_visual_bytes(Limit::Maximum(0));
    let mut format = symphonia::default::get_probe()
        .probe(&hint, media, FormatOptions::default(), metadata)
        .map_err(|error| format!("open {extension} audio: {error}"))?;
    if format.format_info().format != kind.expected_format() {
        return Err("audio container does not match its byte signature".into());
    }
    if format.tracks().len() != 1 {
        return Err("audio decoding requires exactly one audio track".into());
    }
    let track = format
        .first_track(TrackType::Audio)
        .ok_or("audio file has no audio track")?;
    let track_id = track.id;
    let expected_frames = match kind {
        Kind::Mp3 if !mp3_has_declared_length(bytes) => None,
        // ADTS uses a sampled bitrate estimate and WebM timing is not an
        // authoritative PCM count. Their bounded complete framing is checked
        // before probing; decode errors and EOF determine the actual length.
        Kind::AacAdts | Kind::VorbisWebm => None,
        _ => track.num_frames.filter(|frames| *frames > 0),
    };
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or("audio track has no decoder parameters")?;
    parameters::validate(params, kind, expected_frames, target_rate)?;
    let declared_rate = params.sample_rate;
    let declared_channels = params.channels.as_ref().map(|channels| channels.count());
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default().verify(true))
        .map_err(|error| format!("initialize {extension} decoder: {error}"))?;
    let mut source = Vec::<Vec<f32>>::new();
    let mut packet_pcm = Vec::<Vec<f32>>::new();
    let mut rate = None;
    let mut frames = 0usize;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("audio decoding was cancelled".into());
        }
        let Some(packet) = format
            .next_packet()
            .map_err(|error| format!("read {extension} packet: {error}"))?
        else {
            break;
        };
        if packet.track_id != track_id {
            return Err("audio packet belongs to an undeclared track".into());
        }
        // A corrupt packet invalidates the complete AudioBuffer; don't return
        // a plausible but partial decode after Symphonia reports an error.
        let decoded = decoder
            .decode(&packet)
            .map_err(|error| format!("decode {extension} packet: {error}"))?;
        let packet_rate = decoded.spec().rate();
        let channels = decoded.num_planes();
        if !(8_000..=192_000).contains(&packet_rate)
            || !(1..=32).contains(&channels)
            || declared_rate.is_some_and(|rate| rate != packet_rate)
            || declared_channels.is_some_and(|count| count != channels)
            || rate.is_some_and(|previous| previous != packet_rate)
            || (!source.is_empty() && source.len() != channels)
        {
            return Err("audio sample rate or channel count changed".into());
        }
        rate = Some(packet_rate);
        if source.is_empty() {
            source.resize_with(channels, Vec::new);
        }
        frames = frames
            .checked_add(decoded.frames())
            .ok_or("decoded audio size overflow")?;
        let source_bytes = frames
            .checked_mul(channels)
            .and_then(|samples| samples.checked_mul(4))
            .ok_or("decoded audio size overflow")?;
        if source_bytes > MAX_DECODED_BYTES {
            return Err("decoded audio exceeds the 16 MiB AudioBuffer limit".into());
        }
        if frames > 0 {
            resample::output_frames(frames, channels, packet_rate, target_rate)?;
        }
        decoded.copy_to_vecs_planar::<f32>(&mut packet_pcm);
        for (destination, samples) in source.iter_mut().zip(&packet_pcm) {
            destination.extend(samples.iter().map(
                |sample| {
                    if sample.is_finite() { *sample } else { 0.0 }
                },
            ));
        }
    }
    if decoder.finalize().verify_ok == Some(false) {
        return Err("audio checksum verification failed".into());
    }
    if matches!(kind, Kind::FlacOgg) {
        // Zero STREAMINFO samples means unknown length. The Ogg reader may
        // discover a nonzero frame count only when it reaches the EOS granule.
        let final_frames = format
            .tracks()
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.num_frames)
            .filter(|frames| *frames > 0)
            .or(expected_frames);
        if final_frames.is_some_and(|expected| expected != frames as u64) {
            return Err("Ogg/FLAC PCM frame count disagrees with its declared length".into());
        }
    }
    // MP3 encoder delay and AAC priming can make the presented PCM length
    // differ from the container count by a final packet or two. A large
    // deficit still means the supplied complete-file buffer was truncated.
    if let Some(expected) = expected_frames.filter(|expected| frames as u64 + 2_304 < *expected) {
        return Err(format!(
            "audio file ended early (decoded {frames} PCM frames, declared {expected})"
        ));
    }
    let rate = rate.ok_or("audio file produced no PCM")?;
    if let Some(edit) = presentation_edit {
        let window = edit.frame_window(rate)?;
        let start = usize::try_from(window.start).map_err(|_| "audio edit exceeds PCM size")?;
        let end = usize::try_from(window.end).map_err(|_| "audio edit exceeds PCM size")?;
        if start >= end || end > frames {
            return Err("audio edit extends beyond decoded PCM".into());
        }
        for channel in &mut source {
            channel.truncate(end);
            channel.drain(..start);
        }
    }
    resample::finish(source, rate, target_rate, cancelled)
}

#[cfg(test)]
mod tests;
