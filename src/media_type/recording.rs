//! MIME admission for the implemented microphone recording formats.
//! The empty request keeps the existing FLAC default; no playback support is
//! inferred to mean that an encoder or streaming muxer exists.

pub(crate) fn recording_kind(input: &str) -> &'static str {
    if input.is_empty() {
        return "flac";
    }
    let Some(kind) = super::mime::parse(input) else {
        return "";
    };
    match kind.essence.as_str() {
        "audio/flac" if kind.parameters == 0 => "flac",
        "audio/ogg" if kind.parameters == 0 => "opus",
        "audio/webm" if kind.parameters == 0 => "webm-opus",
        "audio/ogg"
            if kind.parameters == 1
                && kind.codecs.as_deref().is_some_and(|codec| {
                    codec.trim_matches([' ', '\t']).eq_ignore_ascii_case("opus")
                }) =>
        {
            "opus"
        }
        "audio/webm"
            if kind.parameters == 1
                && kind.codecs.as_deref().is_some_and(|codec| {
                    codec.trim_matches([' ', '\t']).eq_ignore_ascii_case("opus")
                }) =>
        {
            "webm-opus"
        }
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_mime_admits_only_actual_complete_stream_encoders() {
        for (input, expected) in [
            ("", "flac"),
            (" \t\r\n", ""),
            ("AUDIO/FLAC", "flac"),
            ("audio/ogg", "opus"),
            ("audio/ogg;codecs=opus", "opus"),
            ("audio/ogg;codecs=\" OPUS \"", "opus"),
            ("audio/ogg;codecs=\"op\\us\"", "opus"),
            ("audio/ogg;codecs=;codecs=opus", "opus"),
            ("audio/ogg;codecs=opus;codecs=vorbis", "opus"),
            ("audio/ogg;codecs=vorbis;codecs=opus", ""),
            ("audio/ogg;codecs=opus,vorbis", ""),
            ("audio/ogg;codecs=opus;note=ignored", ""),
            ("audio/ogg;note=\"codecs=opus\"", ""),
            ("audio/flac;codecs=flac", ""),
            ("audio/webm;codecs=opus", "webm-opus"),
            ("audio/opus", ""),
            ("application/ogg;codecs=opus", ""),
            ("video/ogg;codecs=opus", ""),
            ("\u{a0}audio/ogg", ""),
            ("not-mime", ""),
        ] {
            assert_eq!(recording_kind(input), expected, "{input}");
        }
    }
}
