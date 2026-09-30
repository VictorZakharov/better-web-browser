use super::*;

#[test]
fn opus_queries_claim_only_the_admitted_complete_ogg_shape() {
    for (mime, expected) in [
        ("audio/ogg;codecs=opus", "probably"),
        ("Audio/Ogg;CODECS=OPUS", "probably"),
        ("audio/ogg;codecs=\" opus \"", "probably"),
        ("audio/ogg;codecs=opus;codecs=unknown", "probably"),
        ("audio/ogg;codecs=unknown;codecs=opus", ""),
        ("audio/ogg;codecs=opus,vorbis", ""),
        ("audio/ogg;codecs=opus,opus", ""),
        ("audio/ogg;codecs=opus,", ""),
        ("audio/webm;codecs=opus", ""),
        ("audio/mp4;codecs=opus", ""),
        ("video/ogg;codecs=opus", ""),
        ("audio/opus", ""),
    ] {
        assert_eq!(can_play_type(mime), expected, "{mime}");
        assert!(
            !media_source_supported(mime),
            "complete-file is not MSE: {mime}"
        );
    }
}

#[test]
fn opus_capabilities_distinguish_pcm_rate_from_original_input_metadata() {
    for mime in ["audio/ogg;codecs=opus", "Audio/Ogg;codecs=\" OPUS \""] {
        assert_eq!(capability_content_type(mime, "audio", "file"), "supported");
        for channels in ["1", "2"] {
            assert!(audio_parameters_supported(
                mime,
                "file",
                Some(channels),
                Some(48_000.0),
                false
            ));
            for rate in [
                8_000.0, 12_000.0, 16_000.0, 24_000.0, 44_100.0, 96_000.0, 192_000.0,
            ] {
                assert!(!audio_parameters_supported(
                    mime,
                    "file",
                    Some(channels),
                    Some(rate),
                    false
                ));
            }
        }
        assert!(audio_parameters_supported(mime, "file", None, None, false));
        for channels in ["0", "3", "5.1", "stereo"] {
            assert!(!audio_parameters_supported(
                mime,
                "file",
                Some(channels),
                Some(48_000.0),
                false
            ));
        }
        assert!(!audio_parameters_supported(
            mime,
            "file",
            Some("2"),
            Some(48_000.0),
            true
        ));
        assert_eq!(
            capability_content_type(mime, "audio", "media-source"),
            "unsupported"
        );
        assert_eq!(
            capability_content_type(mime, "audio", "webrtc"),
            "unsupported"
        );
        assert_eq!(capability_content_type(mime, "video", "file"), "invalid");
    }
    assert_eq!(
        capability_content_type("audio/webm;codecs=opus", "audio", "file"),
        "unsupported"
    );
}
