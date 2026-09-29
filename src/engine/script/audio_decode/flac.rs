//! Complete native FLAC files for Web Audio's asynchronous `decodeAudioData` path.
//!
//! `claxon` owns the codec bitstream. This wrapper enforces AudioBuffer's
//! document budget while samples are still being produced, including streams
//! whose STREAMINFO does not know their final sample count.

use super::{DecodedAudio, MAX_DECODED_BYTES, MAX_ENCODED_BYTES};
use claxon::{FlacReader, FlacReaderOptions};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn decode(
    bytes: &[u8],
    target_rate: f64,
    cancelled: &AtomicBool,
) -> Result<DecodedAudio, String> {
    if !bytes.starts_with(b"fLaC") || bytes.len() > MAX_ENCODED_BYTES {
        return Err("audio data is not a bounded native FLAC file".into());
    }
    let options = FlacReaderOptions {
        metadata_only: false,
        read_vorbis_comment: false,
    };
    let mut reader = FlacReader::new_ext(Cursor::new(bytes), options)
        .map_err(|error| format!("open FLAC decoder: {error}"))?;
    let info = reader.streaminfo();
    let channels = usize::try_from(info.channels).map_err(|_| "FLAC channel count overflow")?;
    let rate = info.sample_rate;
    let bits = info.bits_per_sample;
    if !(1..=8).contains(&channels)
        || !(8_000..=192_000).contains(&rate)
        || !(4..=32).contains(&bits)
    {
        return Err("unsupported FLAC channel, rate, or sample depth".into());
    }
    // Each eventual AudioBuffer sample is f32. Bound input even when the
    // compressed file or its declared total-samples value is very small.
    let max_samples = MAX_DECODED_BYTES / std::mem::size_of::<f32>();
    if info
        .samples
        .is_some_and(|frames| frames > u64::try_from(max_samples / channels).unwrap_or(u64::MAX))
    {
        return Err("decoded audio exceeds the 16 MiB AudioBuffer limit".into());
    }
    let mut source = vec![Vec::new(); channels];
    let divisor = 2_f64.powi(i32::try_from(bits).map_err(|_| "FLAC sample depth overflow")? - 1);
    let mut samples = 0usize;
    for value in reader.samples() {
        if samples.is_multiple_of(4096) && cancelled.load(Ordering::Relaxed) {
            return Err("audio decoding was cancelled".into());
        }
        if samples >= max_samples {
            return Err("decoded audio exceeds the 16 MiB AudioBuffer limit".into());
        }
        let value = value.map_err(|error| format!("decode FLAC sample: {error}"))?;
        let channel = samples % channels;
        source[channel].push((f64::from(value) / divisor) as f32);
        samples += 1;
    }
    if samples == 0 || !samples.is_multiple_of(channels) {
        return Err("FLAC produced no complete PCM frames".into());
    }
    if info
        .samples
        .is_some_and(|frames| frames != (samples / channels) as u64)
    {
        return Err("FLAC PCM frame count disagrees with STREAMINFO".into());
    }
    super::resample::finish(source, rate, target_rate, cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn fixture() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(
                include_str!("../../../../tests/fixtures/media/test-1s-audio.flac.base64")
                    .lines()
                    .collect::<String>(),
            )
            .expect("decode self-authored FLAC fixture")
    }

    #[test]
    fn decodes_real_flac_pcm_and_resamples_to_context_rate() {
        let bytes = fixture();
        let decoded = decode(&bytes, 44_100.0, &AtomicBool::new(false)).unwrap();
        assert_eq!(decoded.sample_rate, 44_100.0);
        assert_eq!(decoded.channels.len(), 1);
        assert_eq!(decoded.frames, 44_100);
        assert!(decoded.channels[0].iter().any(|sample| *sample > 0.05));
        assert!(decoded.channels[0].iter().any(|sample| *sample < -0.05));

        let resampled = decode(&bytes, 48_000.0, &AtomicBool::new(false)).unwrap();
        assert_eq!(resampled.frames, 48_000);
        assert_eq!(resampled.sample_rate, 48_000.0);
        assert!((resampled.channels[0][0] - decoded.channels[0][0]).abs() < 0.000_001);
    }

    #[test]
    fn malformed_and_cancelled_flac_fail_without_partial_audio() {
        let bytes = fixture();
        assert!(decode(b"not FLAC", 44_100.0, &AtomicBool::new(false)).is_err());
        assert!(decode(&bytes[..8], 44_100.0, &AtomicBool::new(false)).is_err());
        assert!(decode(&bytes, 44_100.0, &AtomicBool::new(true)).is_err());
    }
}
