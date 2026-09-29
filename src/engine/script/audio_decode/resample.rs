use super::{DecodedAudio, MAX_DECODED_BYTES};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn output_frames(
    input_frames: usize,
    channels: usize,
    input_rate: u32,
    target_rate: f64,
) -> Result<usize, String> {
    if input_frames == 0 || channels == 0 || input_rate == 0 || !target_rate.is_finite() {
        return Err("decoded audio has an invalid PCM format".into());
    }
    let frames =
        ((input_frames as f64 * target_rate / f64::from(input_rate)).round() as usize).max(1);
    let bytes = frames
        .checked_mul(channels)
        .and_then(|samples| samples.checked_mul(4))
        .ok_or("decoded audio size overflow")?;
    if bytes > MAX_DECODED_BYTES {
        return Err("decoded audio exceeds the 16 MiB AudioBuffer limit".into());
    }
    Ok(frames)
}

pub(super) fn finish(
    source: Vec<Vec<f32>>,
    input_rate: u32,
    target_rate: f64,
    cancelled: &AtomicBool,
) -> Result<DecodedAudio, String> {
    let input_frames = source.first().map_or(0, Vec::len);
    if source.iter().any(|channel| channel.len() != input_frames) {
        return Err("decoded audio channel lengths disagree".into());
    }
    let output_frames = output_frames(input_frames, source.len(), input_rate, target_rate)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err("audio decoding was cancelled".into());
    }
    let channels = if target_rate == f64::from(input_rate) {
        source
    } else {
        let mut output = vec![Vec::with_capacity(output_frames); source.len()];
        let ratio = f64::from(input_rate) / target_rate;
        for index in 0..output_frames {
            if index.is_multiple_of(4096) && cancelled.load(Ordering::Relaxed) {
                return Err("audio decoding was cancelled".into());
            }
            let position = (index as f64 * ratio).min((input_frames - 1) as f64);
            let first = position.floor() as usize;
            let second = (first + 1).min(input_frames - 1);
            let fraction = (position - first as f64) as f32;
            for (destination, source) in output.iter_mut().zip(&source) {
                destination.push(source[first] * (1.0 - fraction) + source[second] * fraction);
            }
        }
        output
    };
    Ok(DecodedAudio {
        channels,
        sample_rate: target_rate,
        frames: output_frames,
    })
}
