use super::*;

#[test]
fn contained_renderer_loads_plays_and_seeks_adts_aac_lc() {
    verify_audio_source_with_seek(
        220,
        "/tone.aac",
        "audio/aac",
        "audio/aac",
        "AAC-LC",
        decode_base64(include_str!(
            "../../../fixtures/media/test-0.4s-tone.aac.base64"
        )),
        "0.2",
    );
}

#[test]
fn contained_renderer_loads_plays_and_seeks_webm_vorbis() {
    verify_audio_source_with_seek(
        221,
        "/tone.webm",
        "audio/webm; codecs=\"vorbis\"",
        "audio/webm",
        "Vorbis",
        decode_base64(include_str!(
            "../../../fixtures/media/test-0.4s-tone.webm.base64"
        )),
        "0.2",
    );
}

#[test]
fn contained_renderer_loads_plays_and_seeks_ogg_flac() {
    verify_audio_source_with_seek(
        222,
        "/tone.oga",
        "audio/ogg; codecs=\"flac\"",
        "audio/ogg",
        "FLAC",
        decode_base64(include_str!(
            "../../../fixtures/media/test-0.4s-tone.oga.base64"
        )),
        "0.2",
    );
}

#[test]
fn contained_renderer_rejects_video_mixed_webm_without_partial_playback() {
    let encoded = include_str!("../../../fixtures/media/test-0.4s-mixed.webm.base64");
    verify_audio_failure(200, decode_base64(encoded), 4, "audio/webm");
}
