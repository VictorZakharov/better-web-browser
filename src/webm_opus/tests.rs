use super::*;
use base64::Engine;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod bounds;
mod checksum;
mod clusters;
mod durations;
mod fixture_builder;
mod lacing;
mod metadata;
mod presentation;
mod track_options;
mod writer;

fn fixture() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../tests/fixtures/media/test-0.4s-opus.webm.base64")
                .split_whitespace()
                .collect::<String>(),
        )
        .unwrap()
}

#[test]
fn independently_muxed_opus_fixture_has_real_exact_trimmed_pcm() {
    let source = fixture();
    assert!(sniff(&source));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = Stream::open(
        Arc::from(source),
        crate::opus_audio::Limits::default(),
        None,
        deadline,
    )
    .unwrap();
    assert_eq!((stream.channels(), stream.frames()), (1, 19_200));
    let mut pcm = Vec::new();
    while let Some(chunk) = stream.next_pcm(None, deadline).unwrap() {
        pcm.extend(chunk);
    }
    assert_eq!(pcm.len(), 19_200);
    assert!(pcm.iter().all(|value| value.is_finite()));
    assert!(pcm.iter().any(|value| value.abs() > 0.05));
}

#[test]
fn independent_ffmpeg_ogg_and_webm_tones_have_identical_presentation_pcm() {
    let ogg = base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../tests/fixtures/media/test-0.4s-opus.ogg.base64")
                .split_whitespace()
                .collect::<String>(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = crate::opus_audio::Stream::open(
        Arc::from(ogg),
        crate::opus_audio::Limits::default(),
        None,
        deadline,
    )
    .unwrap();
    let mut expected = Vec::new();
    while let Some(chunk) = stream.next_pcm(None, deadline).unwrap() {
        expected.extend(chunk);
    }
    let (channels, frames, actual) = fixture_builder::decode(fixture());
    assert_eq!((channels, frames), (1, 19_200));
    assert_eq!(
        actual, expected,
        "independent container muxes must not expose priming or discard audible samples"
    );
}
