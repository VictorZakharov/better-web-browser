use super::*;

#[test]
fn implemented_audio_containers_skip_unknown_codecs_and_select_the_real_candidate() {
    for (file, mime) in [
        ("tone.aac", "audio/aac;codecs=mp4a.40.2"),
        ("tone.webm", "audio/webm;codecs=vorbis"),
        ("tone.oga", "audio/ogg;codecs=flac"),
    ] {
        let page = Page::parse(
            &format!(
                "<audio><source src='unknown.webm' type='audio/webm;codecs=opus'>\
             <source src='{file}' type='{mime}'><source src='fallback.wav' type='audio/wav'></audio>"
            ),
            "https://example.com/media/",
        );
        assert!(
            matches!(page.resources.as_slice(), [PageResource::Media {url, ..}]
            if url == &format!("https://example.com/media/{file}")),
            "{mime}"
        );
    }
}

#[test]
fn quoted_metadata_cannot_inject_a_source_codec_parameter() {
    let page = Page::parse(
        r#"<audio><source src='tone.webm' type='audio/webm;note="x;codecs=opus";codecs=vorbis'>
             <source src='fallback.mp3' type='audio/mpeg'></audio>"#,
        "https://example.com/media/",
    );
    assert!(
        matches!(page.resources.as_slice(), [PageResource::Media {url, ..}]
        if url == "https://example.com/media/tone.webm")
    );
}

#[test]
fn failed_new_container_advances_once_and_retires_its_old_resource() {
    for (extension, mime) in [
        ("aac", "audio/aac"),
        ("webm", "audio/webm;codecs=vorbis"),
        ("oga", "audio/ogg;codecs=flac"),
    ] {
        let mut page = Page::parse(
            &format!(
                "<audio><source src='broken.{extension}' type='{mime}'>\
             <source src='unknown.webm' type='audio/webm;codecs=opus'>\
             <source src='fallback.mp3' type='audio/mpeg'></audio>"
            ),
            "https://example.com/media/",
        );
        let old = page.resources[0].clone();
        assert_eq!(page.advance_media_source(&old), MediaSourceAdvance::Next);
        assert!(!page.is_current_media_resource(&old));
        assert!(
            matches!(page.resources.as_slice(), [PageResource::Media {url, ..}]
            if url == "https://example.com/media/fallback.mp3")
        );
        let fallback = page.resources[0].clone();
        assert_eq!(page.advance_media_source(&old), MediaSourceAdvance::Stale);
        assert_eq!(page.resources.as_slice(), std::slice::from_ref(&fallback));
        assert_eq!(
            page.advance_media_source(&fallback),
            MediaSourceAdvance::Waiting
        );
        page.refresh_resources(800.0);
        assert!(page.resources.is_empty(), "failed sources must not loop");
    }
}
