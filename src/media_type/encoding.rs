//! Encoding queries share MIME admission with the real incremental recorders.
//! They do not reuse decoder support or initialize hardware/native sessions.

use super::recording_kind;

pub(crate) fn encoding_audio_supported(
    input: &str,
    channels: Option<&str>,
    rate: Option<f64>,
    bitrate: Option<f64>,
) -> bool {
    let kind = recording_kind(input);
    if kind.is_empty()
        || channels.is_some_and(|value| {
            value.is_empty()
                || !value.bytes().all(|byte| byte.is_ascii_digit())
                || !matches!(value.parse::<usize>(), Ok(1 | 2))
        })
    {
        return false;
    }
    if let Some(rate) = rate {
        if !rate.is_finite() || rate.fract() != 0.0 {
            return false;
        }
        let supported = match kind {
            "opus" | "webm-opus" => {
                matches!(rate as u64, 8_000 | 12_000 | 16_000 | 24_000 | 48_000)
            }
            // Match the capture encoder, not the wider complete-file decoder.
            "flac" => (8_000.0..=48_000.0).contains(&rate),
            _ => false,
        };
        if !supported {
            return false;
        }
    }
    // MediaRecorder accepts a bitrate hint, but Opus bounds the actual target.
    // Do not promise that an out-of-range queried target is implemented. FLAC
    // is lossless and accepts hints without promising a fixed achieved rate.
    bitrate.is_none_or(|value| {
        value.is_finite()
            && value.fract() == 0.0
            && value > 0.0
            && (kind == "flac" || (500.0..=512_000.0).contains(&value))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_is_not_inferred_from_the_presence_of_a_decoder() {
        for input in [
            "audio/webm;codecs=vorbis",
            "audio/ogg;codecs=vorbis",
            "audio/mpeg",
            "audio/aac",
            "video/mp4;codecs=avc1.42e01e",
            "video/webm;codecs=vp8,opus",
            "audio/opus",
            "audio/mp4;codecs=opus",
        ] {
            assert!(
                !encoding_audio_supported(input, None, None, None),
                "{input}"
            );
        }
        for input in [
            "audio/flac",
            "audio/ogg;codecs=opus",
            "audio/webm;codecs=opus",
        ] {
            assert!(encoding_audio_supported(input, None, None, None));
        }
    }

    #[test]
    fn opus_capture_rates_channels_and_targets_match_actual_encoder_bounds() {
        for input in ["audio/ogg;codecs=opus", "audio/webm;codecs=opus"] {
            for channels in ["1", "2", "02"] {
                for rate in [8_000.0, 12_000.0, 16_000.0, 24_000.0, 48_000.0] {
                    for bitrate in [500.0, 64_000.0, 512_000.0] {
                        assert!(encoding_audio_supported(
                            input,
                            Some(channels),
                            Some(rate),
                            Some(bitrate)
                        ));
                    }
                }
            }
            for channels in ["", "0", "3", "stereo", "5.1", "-1", " 2", "2x"] {
                assert!(!encoding_audio_supported(input, Some(channels), None, None));
            }
            for rate in [0.0, 44_100.0, 48_000.5, 96_000.0, f64::NAN, f64::INFINITY] {
                assert!(!encoding_audio_supported(input, None, Some(rate), None));
            }
            for bitrate in [0.0, 499.0, 512_001.0, -1.0, 500.5, f64::NAN] {
                assert!(!encoding_audio_supported(input, None, None, Some(bitrate)));
            }
        }
    }

    #[test]
    fn lossless_capture_has_its_own_sample_rate_and_bitrate_hint_policy() {
        for rate in [8_000.0, 44_100.0, 48_000.0] {
            assert!(encoding_audio_supported(
                "audio/flac",
                Some("2"),
                Some(rate),
                Some(128_000.0)
            ));
        }
        for rate in [7_999.0, 48_001.0, 96_000.0, 192_000.0, 200_001.0, f64::NAN] {
            assert!(!encoding_audio_supported(
                "audio/flac",
                None,
                Some(rate),
                None
            ));
        }
        assert!(!encoding_audio_supported(
            "audio/flac",
            Some("3"),
            None,
            None
        ));
    }
}
