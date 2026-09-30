//! Bound declared codec allocation before constructing a native decoder.

use super::{Kind, MAX_DECODED_BYTES, resample};
use symphonia::core::codecs::audio::AudioCodecParameters;

pub(super) fn validate(
    params: &AudioCodecParameters,
    kind: Kind,
    expected_frames: Option<u64>,
    target_rate: f64,
) -> Result<(), String> {
    if params.codec != kind.expected_codec() {
        return Err("audio track codec is unsupported for this container".into());
    }
    if params
        .sample_rate
        .is_some_and(|rate| !(8_000..=192_000).contains(&rate))
        || params
            .channels
            .as_ref()
            .is_some_and(|channels| !(1..=32).contains(&channels.count()))
    {
        return Err("audio track exceeds the supported PCM format".into());
    }
    if params
        .extra_data
        .as_ref()
        .is_some_and(|data| data.len() > 64 * 1024)
    {
        return Err("audio codec initialization data exceeds the 64 KiB limit".into());
    }
    if let Some(channels) = params.channels.as_ref() {
        // A short declared duration must not hide an oversized packet allocation.
        for frames in [expected_frames, params.max_frames_per_packet]
            .into_iter()
            .flatten()
        {
            if frames > (MAX_DECODED_BYTES / (channels.count() * 4)) as u64 {
                return Err("decoded audio exceeds the 16 MiB AudioBuffer limit".into());
            }
            if let Some(rate) = params.sample_rate
                && frames > 0
            {
                resample::output_frames(frames as usize, channels.count(), rate, target_rate)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::audio::Channels;

    fn params() -> AudioCodecParameters {
        let mut params = AudioCodecParameters::new();
        params
            .for_codec(Kind::AacAdts.expected_codec())
            .with_sample_rate(44_100)
            .with_channels(Channels::Discrete(1));
        params
    }

    #[test]
    fn declared_codec_rate_channels_and_initialization_data_are_bounded() {
        let valid = params();
        assert!(validate(&valid, Kind::AacAdts, Some(17_640), 48_000.0).is_ok());
        assert!(validate(&valid, Kind::VorbisWebm, None, 48_000.0).is_err());
        for rate in [0, 7_999, 192_001, u32::MAX] {
            let mut invalid = valid.clone();
            invalid.sample_rate = Some(rate);
            assert!(validate(&invalid, Kind::AacAdts, None, 48_000.0).is_err());
        }
        for count in [0, 33, u16::MAX] {
            let mut invalid = valid.clone();
            invalid.channels = Some(Channels::Discrete(count));
            assert!(validate(&invalid, Kind::AacAdts, None, 48_000.0).is_err());
        }
        let mut invalid = valid;
        invalid.extra_data = Some(vec![0; 64 * 1024 + 1].into_boxed_slice());
        assert!(validate(&invalid, Kind::AacAdts, None, 48_000.0).is_err());
    }

    #[test]
    fn declared_source_packet_and_resampled_output_cannot_exceed_pcm_budget() {
        let mut params = params();
        let limit = (MAX_DECODED_BYTES / 4) as u64;
        assert!(validate(&params, Kind::AacAdts, Some(limit + 1), 44_100.0).is_err());
        params.max_frames_per_packet = Some(limit + 1);
        assert!(validate(&params, Kind::AacAdts, Some(1), 44_100.0).is_err());
        params.max_frames_per_packet = None;
        params.sample_rate = Some(8_000);
        assert!(validate(&params, Kind::AacAdts, Some(limit / 2), 192_000.0).is_err());
    }
}
