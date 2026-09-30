use super::*;
mod opus;

#[test]
fn only_implemented_container_codec_combinations_are_admitted() {
    for (mime, expected) in [
        ("audio/aac", "maybe"),
        ("audio/aac;codecs=mp4a.40.2", "probably"),
        ("audio/aac;codecs=mp4a.40.5", ""),
        ("audio/webm", "maybe"),
        ("audio/webm;codecs=vorbis", "probably"),
        ("audio/webm;codecs=opus", ""),
        ("audio/webm;codecs=vp8,vorbis", ""),
        ("video/webm", ""),
        ("video/webm;codecs=vorbis", ""),
        ("audio/ogg;codecs=flac", "probably"),
        ("audio/ogg;codecs=vorbis", "probably"),
        ("audio/ogg;codecs=opus", "probably"),
        ("audio/ogg;codecs=vorbis,flac", ""),
        ("audio/ogg;codecs=flac,unknown", ""),
        ("audio/mpeg;codecs=mp3", "probably"),
        ("audio/mp4;codecs=avc1.42e01e", ""),
        ("audio/mp4;codecs=avc1.42e01e,mp4a.40.2", ""),
        ("video/mp4;codecs=avc1.42e01e", "probably"),
        ("video/mp4;codecs=avc1.42e01e,mp4a.40.2", "probably"),
        ("video/mp4;codecs=avc1.42e01e,unknown", ""),
        ("audio/flac", "maybe"),
        ("audio/flac;codecs=flac", ""),
        ("application/octet-stream", ""),
    ] {
        assert_eq!(can_play_type(mime), expected, "{mime}");
    }
}

#[test]
fn owned_mp4_wave_and_flac_aliases_remain_consistent() {
    for essence in ["audio/mp4", "audio/m4a", "audio/x-m4a", "application/mp4"] {
        assert_eq!(can_play_type(essence), "maybe");
        assert_eq!(
            can_play_type(&format!("{essence};codecs=mp4a.40.2")),
            "probably"
        );
        assert_eq!(can_play_type(&format!("{essence};codecs=mp4a.40.5")), "");
    }
    for essence in ["audio/wav", "audio/wave", "audio/x-wav", "audio/vnd.wave"] {
        assert_eq!(can_play_type(essence), "maybe");
        for codec in ["1", "pcm"] {
            assert_eq!(
                can_play_type(&format!("{essence};codecs={codec}")),
                "probably"
            );
        }
        assert_eq!(can_play_type(&format!("{essence};codecs=mp3")), "");
    }
    for essence in ["audio/flac", "audio/x-flac"] {
        assert_eq!(can_play_type(essence), "maybe");
        assert_eq!(
            can_play_type(&format!("{essence};unused=ordinary")),
            "maybe"
        );
    }
}

#[test]
fn mime_parameters_use_first_valid_value_and_ignore_unrelated_quoted_codecs() {
    for (mime, expected) in [
        (" Audio/WebM \t;CODECS=vorbis", "probably"),
        ("audio/webm;codecs=vorbis;codecs=opus", "probably"),
        ("audio/webm;codecs=opus;codecs=vorbis", ""),
        ("audio/webm;codecs=;codecs=vorbis", "probably"),
        ("audio/webm;codecs=\u{100};codecs=vorbis", "probably"),
        ("audio/webm;codecs=\"\";codecs=vorbis", ""),
        (
            "audio/webm;note=\"x;codecs=opus\";codecs=vorbis",
            "probably",
        ),
        ("audio/webm;codecs=\"vor\\bis\"", "probably"),
        ("audio/webm;codecs=\"vorbis\"ignored;other=1", "probably"),
        ("audio/webm;codecs=\"vorbis", "probably"),
        ("audio/webm;codecs='vorbis'", ""),
        ("audio/webm;codecs=vorbis,", ""),
        ("audio/webm;codecs", "maybe"),
        ("audio/webm;codecs =vorbis", "maybe"),
        ("audio/webm;codecs= vorbis", "probably"),
        ("audio/webm;codecs=vorbis\t", "probably"),
        ("audio/webm;codecs=\u{a0}vorbis", ""),
    ] {
        assert_eq!(can_play_type(mime), expected, "{mime}");
    }
}

#[test]
fn complete_file_support_does_not_grant_iso_bmff_sourcebuffer_support() {
    for mime in [
        "audio/aac;codecs=mp4a.40.2",
        "audio/webm;codecs=vorbis",
        "audio/ogg;codecs=flac",
        "audio/ogg;codecs=vorbis",
        "audio/ogg;codecs=opus",
        "audio/mpeg;codecs=mp3",
        "audio/flac",
        "audio/mp4",
        "audio/mp4;codecs=mp4a.40.5",
        "audio/mp4;codecs=avc1.42e01e",
        "audio/mp4;codecs=avc1.42e01e,mp4a.40.2",
        "audio/m4a;codecs=mp4a.40.2",
    ] {
        assert!(!media_source_supported(mime), "{mime}");
    }
    for mime in [
        "audio/mp4;codecs=mp4a.40.2",
        "video/mp4;codecs=avc1.42e01e",
        "video/mp4;codecs=avc1.42e01e,mp4a.40.2",
        "application/mp4;codecs=mp4a.40.2",
    ] {
        assert!(media_source_supported(mime), "{mime}");
    }
}

#[test]
fn malformed_mime_essences_do_not_claim_a_decoder() {
    for mime in [
        "",
        "/webm",
        "audio/",
        "audio /webm",
        "audio/ webm",
        "audio/webm/extra",
        "audio/webm\0",
        "audio/💡",
        "\u{a0}audio/webm",
        "audio/webm\u{c}",
    ] {
        assert_eq!(can_play_type(mime), "", "{mime:?}");
    }
    assert_eq!(can_play_type("\r\n\taudio/webm\r\n"), "maybe");
    assert_eq!(can_play_type("audio/webm;;;"), "maybe");
}

#[test]
fn sourcebuffer_track_configuration_ignores_quoted_non_codec_parameters() {
    for (mime, expected) in [
        ("audio/mp4;note=\"avc1.42e01e\";codecs=mp4a.40.2", "audio"),
        ("video/mp4;note=\"mp4a.40.2\";codecs=avc1.42e01e", "video"),
        ("video/mp4;codecs=avc1.42e01e,mp4a.40.2", "muxed"),
        ("video/mp4;codecs=avc1.42e01e;codecs=mp4a.40.2", "video"),
        ("video/mp4;codecs=mp4a.40.2;codecs=avc1.42e01e", "audio"),
        ("audio/aac;codecs=mp4a.40.2", ""),
        ("video/mp4;note=avc1.42e01e", ""),
    ] {
        assert_eq!(media_source_track_kind(mime), expected, "{mime}");
    }
}
