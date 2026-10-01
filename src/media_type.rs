//! One playback support matrix for `<source type>` and HTMLMediaElement.canPlayType.
//!
//! Complete-file decoding does not imply MediaSource or MediaRecorder support.
//! Unknown codecs must never be admitted because another track happens to be supported.

mod capabilities;
mod encoding;
mod mime;
mod recording;

pub(crate) use capabilities::{audio_parameters_supported, capability_content_type};
pub(crate) use encoding::encoding_audio_supported;
pub(crate) use recording::recording_kind;
#[cfg(test)]
mod tests;

/// HTML's three-valued answer describes a container/codec combination, not
/// whether a particular file is valid. The contained decoder still checks bytes.
pub(crate) fn can_play_type(input: &str) -> &'static str {
    let Some(kind) = mime::parse(input) else {
        return "";
    };
    let essence = kind.essence.as_str();
    let wave = matches!(
        essence,
        "audio/wav" | "audio/wave" | "audio/x-wav" | "audio/vnd.wave"
    );
    let mpeg = essence == "audio/mpeg";
    let aac = essence == "audio/aac";
    let ogg = essence == "audio/ogg";
    let webm = essence == "audio/webm";
    let flac = matches!(essence, "audio/flac" | "audio/x-flac");
    let mp4 = matches!(
        essence,
        "video/mp4" | "audio/mp4" | "audio/m4a" | "audio/x-m4a" | "application/mp4"
    );
    if !wave && !mpeg && !aac && !ogg && !webm && !flac && !mp4 {
        return "";
    }
    let Some(codecs) = kind.codecs else {
        // All admitted containers have real decoding, but an unspecified codec
        // may be unsupported. Audio-only WebM does not imply video or multiplexing.
        return "maybe";
    };
    let codecs = codecs.to_ascii_lowercase();
    let codecs: Vec<_> = codecs
        .split(',')
        .map(|codec| codec.trim_matches([' ', '\t']))
        .collect();
    if codecs.iter().any(|codec| codec.is_empty()) {
        return "";
    }
    let supported = if wave {
        codecs.len() == 1 && matches!(codecs[0], "1" | "pcm")
    } else if mpeg {
        codecs == ["mp3"]
    } else if aac {
        codecs == ["mp4a.40.2"]
    } else if ogg {
        codecs.len() == 1 && matches!(codecs[0], "vorbis" | "flac" | "opus")
    } else if webm {
        codecs.len() == 1 && matches!(codecs[0], "vorbis" | "opus")
    } else if flac {
        // RFC 9639's native FLAC media type defines no codec parameter.
        false
    } else {
        let audio = codecs.contains(&"mp4a.40.2");
        let video = codecs.iter().any(|codec| {
            codec.strip_prefix("avc1.").is_some_and(|hex| {
                hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        });
        (audio || video)
            && (!video || !essence.starts_with("audio/"))
            && codecs.len() == usize::from(audio) + usize::from(video)
    };
    if supported { "probably" } else { "" }
}

/// The current SourceBuffer parser consumes complete ISO-BMFF segments only.
/// A real complete-file AAC decoder must not make raw ADTS appendable to MSE.
pub(crate) fn media_source_supported(input: &str) -> bool {
    !media_source_track_kind(input).is_empty()
}

/// Classify accepted SourceBuffer codecs, not text inside unrelated parameters.
pub(crate) fn media_source_track_kind(input: &str) -> &'static str {
    let Some(kind) = mime::parse(input) else {
        return "";
    };
    if !matches!(
        kind.essence.as_str(),
        "video/mp4" | "audio/mp4" | "application/mp4"
    ) || can_play_type(input) != "probably"
    {
        return "";
    }
    let codecs = kind.codecs.unwrap_or_default().to_ascii_lowercase();
    let audio = codecs
        .split(',')
        .any(|codec| codec.trim_matches([' ', '\t']) == "mp4a.40.2");
    let video = codecs
        .split(',')
        .any(|codec| codec.trim_matches([' ', '\t']).starts_with("avc1."));
    match (audio, video) {
        (true, true) => "muxed",
        (true, false) => "audio",
        (false, true) => "video",
        _ => "",
    }
}
