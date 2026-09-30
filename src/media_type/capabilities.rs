//! MediaCapabilities validates one track, unlike canPlayType's container query.
//! https://w3c.github.io/media-capabilities/#check-mime-type-validity

use super::{can_play_type, media_source_supported, mime};

const LOW_MPEG_SAMPLE_RATES: [u64; 9] = [
    8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000,
];

pub(crate) fn capability_content_type(input: &str, media: &str, mode: &str) -> &'static str {
    let Some(kind) = mime::parse(input) else {
        return "invalid";
    };
    if !matches!(media, "audio" | "video") {
        return "invalid";
    }
    let major = kind.essence.split('/').next().unwrap_or_default();
    if major != media && major != "application" {
        return "invalid";
    }
    let single = matches!(
        kind.essence.as_str(),
        "audio/aac" | "audio/mpeg" | "audio/flac" | "audio/x-flac"
    );
    let multiple = matches!(
        kind.essence.as_str(),
        "audio/webm"
            | "video/webm"
            | "audio/ogg"
            | "video/ogg"
            | "application/ogg"
            | "audio/mp4"
            | "video/mp4"
            | "application/mp4"
            | "audio/m4a"
            | "audio/x-m4a"
            | "audio/wav"
            | "audio/wave"
            | "audio/x-wav"
            | "audio/vnd.wave"
    );
    if single && kind.parameters != 0 {
        return "invalid";
    }
    if multiple {
        let Some(codec) = kind.codecs.as_deref() else {
            return "invalid";
        };
        let codec = codec.trim_matches([' ', '\t']).to_ascii_lowercase();
        if kind.parameters != 1
            || codec.is_empty()
            || codec.contains(',')
            || codec_media(&codec).is_some_and(|codec_media| codec_media != media)
        {
            return "invalid";
        }
    }
    let supported = match mode {
        "file" => !can_play_type(input).is_empty(),
        "media-source" => media_source_supported(input),
        _ => false,
    };
    if supported {
        "supported"
    } else {
        "unsupported"
    }
}

fn codec_media(codec: &str) -> Option<&'static str> {
    if matches!(
        codec,
        "vorbis" | "opus" | "flac" | "mp3" | "pcm" | "1" | "ac-3" | "ec-3"
    ) || codec.starts_with("mp4a.")
    {
        Some("audio")
    } else if matches!(codec, "vp8" | "vp9" | "theora")
        || ["avc1.", "avc3.", "hvc1.", "hev1.", "vp09.", "av01."]
            .iter()
            .any(|prefix| codec.starts_with(prefix))
    {
        Some("video")
    } else {
        None
    }
}

/// Only formats positively established by the contained playback path are
/// admitted. Unknown channel-layout names are not inferred to mean stereo.
pub(crate) fn audio_parameters_supported(
    input: &str,
    mode: &str,
    channels: Option<&str>,
    sample_rate: Option<f64>,
    spatial: bool,
) -> bool {
    if spatial
        || channels.is_some_and(|value| {
            !value.bytes().all(|byte| byte.is_ascii_digit())
                || !matches!(value.parse::<u32>(), Ok(1 | 2))
        })
    {
        return false;
    }
    let Some(rate) = sample_rate else {
        return true;
    };
    if !rate.is_finite() || rate.fract() != 0.0 {
        return false;
    }
    let Some(kind) = mime::parse(input) else {
        return false;
    };
    let rate = rate as u64;
    if mode == "media-source" {
        // Fragmented AAC uses Media Foundation, not the complete-file decoder.
        // Its documented output contract is indexed 8–48 kHz.
        // https://learn.microsoft.com/en-us/windows/win32/medfound/aac-decoder#format-constraints
        return matches!(kind.essence.as_str(), "audio/mp4" | "application/mp4")
            && LOW_MPEG_SAMPLE_RATES.contains(&rate);
    }
    match kind.essence.as_str() {
        "audio/aac" => [
            8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 64_000, 88_200,
            96_000,
        ]
        .contains(&rate),
        "audio/mpeg" => LOW_MPEG_SAMPLE_RATES.contains(&rate),
        // A decoder-only STREAMINFO ceiling is not an output capability:
        // keep FLAC within our 8–192 kHz PCM policy and XAudio2 voice limits.
        "audio/flac" | "audio/x-flac" => (8_000..=192_000).contains(&rate),
        _ => (8_000..=192_000).contains(&rate),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_queries_validate_track_mime_separately_from_support() {
        for (input, media, expected) in [
            ("audio/webm;codecs=vorbis", "audio", "supported"),
            ("audio/webm;codecs=opus", "audio", "unsupported"),
            ("video/webm;codecs=vp8", "video", "unsupported"),
            ("audio/webm;codecs=vorbis", "video", "invalid"),
            ("video/mp4;codecs=mp4a.40.2", "video", "invalid"),
            ("application/mp4;codecs=mp4a.40.2", "audio", "supported"),
            ("application/mp4;codecs=avc1.42e01e", "audio", "invalid"),
            ("application/mp4;codecs=avc1.42e01e", "video", "supported"),
            ("audio/aac", "audio", "supported"),
            ("audio/mpeg", "audio", "supported"),
            ("audio/flac", "audio", "supported"),
            ("audio/mpeg;codecs=mp3", "audio", "invalid"),
            ("audio/flac;note=ignored", "audio", "invalid"),
            ("audio/webm", "audio", "invalid"),
            ("audio/webm;codecs=vorbis;note=ignored", "audio", "invalid"),
            ("audio/webm;codecs=vorbis,opus", "audio", "invalid"),
            ("audio/webm;codecs=vorbis;codecs=opus", "audio", "supported"),
            ("audio/webm;codecs=;codecs=vorbis", "audio", "supported"),
            ("audio/unknown", "audio", "unsupported"),
            ("not-mime", "audio", "invalid"),
        ] {
            assert_eq!(
                capability_content_type(input, media, "file"),
                expected,
                "{input}"
            );
        }
        assert_eq!(
            capability_content_type("audio/webm;codecs=vorbis", "audio", "media-source"),
            "unsupported"
        );
    }

    #[test]
    fn audio_queries_do_not_claim_unsupported_channel_layouts_or_source_rates() {
        for channels in [
            "0",
            "3",
            "8",
            "5.1",
            "stereo",
            "2.0",
            "",
            "-1",
            "2x",
            "4294967296",
        ] {
            assert!(!audio_parameters_supported(
                "audio/webm",
                "file",
                Some(channels),
                Some(44_100.0),
                false
            ));
        }
        for rate in [0.0, 7_999.0, 192_001.0, f64::INFINITY, f64::NAN, 44_100.5] {
            assert!(!audio_parameters_supported(
                "audio/webm",
                "file",
                Some("2"),
                Some(rate),
                false
            ));
        }
        assert!(audio_parameters_supported(
            "audio/webm",
            "file",
            Some("2"),
            Some(192_000.0),
            false
        ));
        assert!(audio_parameters_supported(
            "audio/webm",
            "file",
            None,
            None,
            false
        ));
        assert!(!audio_parameters_supported(
            "audio/webm",
            "file",
            Some("2"),
            Some(44_100.0),
            true
        ));
        assert!(!audio_parameters_supported(
            "audio/aac",
            "file",
            Some("1"),
            Some(192_000.0),
            false
        ));
        assert!(!audio_parameters_supported(
            "audio/mpeg",
            "file",
            Some("2"),
            Some(96_000.0),
            false
        ));
        for mime in ["audio/flac", "audio/x-flac"] {
            for rate in [8_000.0, 192_000.0] {
                assert!(audio_parameters_supported(
                    mime,
                    "file",
                    Some("2"),
                    Some(rate),
                    false
                ));
            }
            for rate in [7_999.0, 192_001.0, 200_001.0, 384_000.0] {
                assert!(!audio_parameters_supported(
                    mime,
                    "file",
                    Some("1"),
                    Some(rate),
                    false
                ));
            }
        }
    }

    #[test]
    fn media_source_aac_does_not_inherit_complete_file_explicit_sample_rates() {
        for mime in [
            "audio/mp4;codecs=mp4a.40.2",
            "application/mp4;codecs=mp4a.40.2",
        ] {
            assert!(audio_parameters_supported(
                mime,
                "file",
                Some("2"),
                Some(192_000.0),
                false
            ));
            assert!(audio_parameters_supported(
                mime,
                "media-source",
                None,
                None,
                false
            ));
            for rate in LOW_MPEG_SAMPLE_RATES {
                assert!(audio_parameters_supported(
                    mime,
                    "media-source",
                    Some("2"),
                    Some(rate as f64),
                    false
                ));
            }
            for rate in [32_001.0, 64_000.0, 96_000.0, 192_000.0] {
                assert!(!audio_parameters_supported(
                    mime,
                    "media-source",
                    Some("1"),
                    Some(rate),
                    false
                ));
            }
        }
    }
}
