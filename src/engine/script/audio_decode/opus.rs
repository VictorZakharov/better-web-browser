//! Complete Ogg/WebM Opus presentation PCM for document-owned Web Audio jobs.

use super::{DecodedAudio, MAX_DECODED_BYTES, MAX_ENCODED_BYTES, resample};
use crate::limits::MEDIA_COMMAND_TIMEOUT;
use crate::opus_audio::{Limits, SAMPLE_RATE, Stream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub(super) fn decode(
    bytes: &[u8],
    target_rate: f64,
    cancelled: &AtomicBool,
) -> Result<DecodedAudio, String> {
    if bytes.is_empty() || bytes.len() > MAX_ENCODED_BYTES {
        return Err("encoded Opus audio exceeds the 8 MiB decode limit".into());
    }
    let deadline = Instant::now() + MEDIA_COMMAND_TIMEOUT;
    let limits = Limits {
        max_decoded_bytes: MAX_DECODED_BYTES,
        ..Limits::default()
    };
    let mut stream = Stream::open(Arc::from(bytes), limits, Some(cancelled), deadline)?;
    let channels = usize::from(stream.channels());
    let expected_frames = usize::try_from(stream.frames())
        .map_err(|_| "decoded Opus frame count is not representable")?;
    if !(1..=2).contains(&channels)
        || expected_frames
            .checked_mul(channels)
            .and_then(|samples| samples.checked_mul(4))
            .is_none_or(|bytes| bytes > MAX_DECODED_BYTES)
    {
        return Err("decoded Opus exceeds the 16 MiB AudioBuffer limit".into());
    }
    // Container admission removes pre-skip and Ogg EOS/WebM DiscardPadding.
    // Only presentation samples enter AudioBuffer/context-rate conversion.
    resample::output_frames(expected_frames, channels, SAMPLE_RATE, target_rate)?;
    let mut source = (0..channels)
        .map(|_| Vec::<f32>::with_capacity(expected_frames))
        .collect::<Vec<_>>();
    let mut frames = 0usize;
    while let Some(samples) = stream.next_pcm(Some(cancelled), deadline)? {
        if cancelled.load(Ordering::Relaxed) {
            return Err("audio decoding was cancelled".into());
        }
        if samples.is_empty() || !samples.len().is_multiple_of(channels) {
            return Err("decoded Opus PCM channel alignment is invalid".into());
        }
        frames = frames
            .checked_add(samples.len() / channels)
            .filter(|frames| *frames <= expected_frames)
            .ok_or("decoded Opus exceeded its presentation frame count")?;
        for frame in samples.chunks_exact(channels) {
            for (destination, sample) in source.iter_mut().zip(frame) {
                if !sample.is_finite() {
                    return Err("decoded Opus produced a non-finite sample".into());
                }
                destination.push(*sample);
            }
        }
    }
    if frames != expected_frames {
        return Err("decoded Opus ended before its presentation frame count".into());
    }
    resample::finish(source, SAMPLE_RATE, target_rate, cancelled)
}

#[cfg(test)]
mod tests;
