use super::*;
use crate::engine::Page;

#[path = "media_sources_tests/containers.rs"]
mod containers;

#[test]
fn audio_source_skips_unsupported_codec_and_media_query() {
    let page = Page::parse(
        r#"<audio><source src="hidden.mp3" type="audio/mpeg" media="print">
                 <source src="he.aac" type='audio/mp4; codecs="mp4a.40.5"'>
                 <source src="song.mp3" type='audio/mpeg; codecs="mp3"'>
                 <source src="fallback.wav" type="audio/vnd.wave"></audio>
           <audio src="second.aac"></audio>"#,
        "https://example.com/music/",
    );
    let media = page
        .resources
        .iter()
        .filter_map(|resource| match resource {
            PageResource::Media { url, .. } => Some(url.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(media, ["https://example.com/music/song.mp3"]);
}

#[test]
fn direct_audio_src_is_discovered() {
    let page = Page::parse(
        r#"<audio src="song.aac"></audio>"#,
        "https://example.com/music/",
    );
    assert!(matches!(page.resources.as_slice(),
        [PageResource::Media { url, .. }] if url == "https://example.com/music/song.aac"));
}

#[test]
fn m4a_alias_selects_aac_source_but_not_video_only_codec() {
    assert!(supported_media_type("audio/m4a; codecs=mp4a.40.2"));
    assert!(supported_media_type("audio/x-m4a; codecs=mp4a.40.2"));
    assert!(!supported_media_type("audio/m4a; codecs=avc1.42e01e"));
    let page = Page::parse(
        r#"<audio><source src="video.m4a" type='audio/x-m4a; codecs="avc1.42e01e"'>
                 <source src="song.m4a" type='audio/x-m4a; codecs="mp4a.40.2"'>
                 <source src="fallback.mp4" type="audio/mp4"></audio>"#,
        "https://example.com/music/",
    );
    assert!(matches!(page.resources.as_slice(),
        [PageResource::Media { url, .. }] if url == "https://example.com/music/song.m4a"));
}

#[test]
fn native_flac_source_still_skips_unknown_ogg_codec() {
    let page = Page::parse(
        r#"<audio><source src="skip.ogg" type='audio/ogg; codecs="opus"'>
                 <source src="skip.flac" type='audio/flac; codecs="flac"'>
                 <source src="song.flac" type="audio/flac">
                 <source src="fallback.mp3" type="audio/mpeg"></audio>"#,
        "https://example.com/music/",
    );
    assert!(matches!(page.resources.as_slice(),
        [PageResource::Media { url, .. }] if url == "https://example.com/music/song.flac"));
}

#[test]
fn ogg_vorbis_source_is_selected_without_claiming_other_ogg_codecs() {
    let page = Page::parse(
        r#"<audio><source src="skip.ogg" type='audio/ogg; codecs="opus"'>
                 <source src="song.ogg" type='audio/ogg; codecs="vorbis"'>
                 <source src="fallback.mp3" type="audio/mpeg"></audio>"#,
        "https://example.com/music/",
    );
    assert!(matches!(page.resources.as_slice(),
        [PageResource::Media { url, .. }] if url == "https://example.com/music/song.ogg"));
}

#[test]
fn video_only_mp4_source_is_selected_without_an_aac_track() {
    let page = Page::parse(
        "<video><source src='silent.mp4' type='video/mp4; codecs=\"avc1.42e01e\"'>\
         <source src='fallback.mp4' type='video/mp4'></video>",
        "https://example.com/watch/",
    );
    assert!(matches!(page.resources.as_slice(),
        [PageResource::Media { url, .. }] if url == "https://example.com/watch/silent.mp4"));
}

#[test]
fn failed_child_source_advances_once_and_waits_after_last_candidate() {
    let mut page = Page::parse(
        "<audio><source src='broken.mp3'><source src='fallback.mp3'></audio>",
        "https://example.com/music/",
    );
    let first = page.resources[0].clone();
    assert_eq!(page.advance_media_source(&first), MediaSourceAdvance::Next);
    assert!(!page.is_current_media_resource(&first));
    let fallback = page.resources[0].clone();
    assert!(matches!(&fallback, PageResource::Media { url, .. }
        if url == "https://example.com/music/fallback.mp3"));
    assert_eq!(
        page.advance_media_source(&fallback),
        MediaSourceAdvance::Waiting
    );
    assert!(page.resources.is_empty());
    page.refresh_resources(800.0);
    assert!(
        page.resources.is_empty(),
        "refresh must not retry failed candidates"
    );
}

#[test]
fn src_replacement_invalidates_old_response_even_before_refresh() {
    let mut page = Page::parse(
        "<audio src='first.mp3'></audio>",
        "https://example.com/music/",
    );
    let old = page.resources[0].clone();
    let audio = page.dom.elements_named("audio").next().unwrap();
    audio.set_attr("src", "second.mp3");
    assert!(!page.is_current_media_resource(&old));
    page.refresh_resources(800.0);
    let second = page.resources[0].clone();
    assert!(page.is_current_media_resource(&second));
    audio.set_attr("src", "first.mp3");
    page.refresh_resources(800.0);
    let replacement = &page.resources[0];
    assert_ne!(
        replacement, &old,
        "returning to an old URL needs a new fetch"
    );
    assert!(page.is_current_media_resource(replacement));
}

#[test]
fn removing_src_keeps_the_selected_resource_until_load_is_reinvoked() {
    let mut page = Page::parse(
        "<audio src='first.mp3'><source src='fallback.mp3'></audio>",
        "https://example.com/music/",
    );
    let selected = page.resources[0].clone();
    page.dom
        .elements_named("audio")
        .next()
        .unwrap()
        .remove_attr("src");
    page.refresh_resources(800.0);
    assert_eq!(page.resources, [selected]);
}

#[test]
fn explicit_load_reselects_unchanged_source_with_fresh_request_identity() {
    let mut page = Page::parse(
        "<audio><source src='same.mp3'></audio>",
        "https://example.com/music/",
    );
    let first = page.resources[0].clone();
    let node = page.dom.elements_named("audio").next().unwrap().id();
    page.reload_media_source(node);
    let replacement = page.resources[0].clone();
    assert_ne!(
        first, replacement,
        "load() must bypass the loaded-resource cache"
    );
    assert!(!page.is_current_media_resource(&first));
    assert!(page.is_current_media_resource(&replacement));
    assert!(matches!(replacement, PageResource::Media { url, .. }
        if url == "https://example.com/music/same.mp3"));
}

#[test]
fn media_crossorigin_uses_potential_cors_request_options() {
    let options = [
        "<audio src='a.mp3'></audio>",
        "<audio crossorigin src='b.mp3'></audio>",
        "<audio crossorigin='use-credentials' src='c.mp3'></audio>",
    ]
    .into_iter()
    .map(|html| {
        let page = Page::parse(html, "https://example.com/music/");
        let PageResource::Media {
            mode, credentials, ..
        } = &page.resources[0]
        else {
            panic!("audio resource")
        };
        (*mode, *credentials)
    })
    .collect::<Vec<_>>();
    assert_eq!(
        options,
        [
            (RequestMode::NoCors, CredentialsMode::Include),
            (RequestMode::Cors, CredentialsMode::SameOrigin),
            (RequestMode::Cors, CredentialsMode::Include),
        ]
    );
}
