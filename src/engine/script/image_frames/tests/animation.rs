use super::super::test_fixtures;
use super::*;
mod metadata;

#[test]
fn gif_partial_rectangles_and_previous_disposal_are_composed_by_the_codec() {
    let result = codecs::decode(
        "image/gif",
        &test_fixtures::disposing_gif(),
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result.frames.len(), 3);
    let red = [255, 0, 0, 255];
    let green = [0, 255, 0, 255];
    let blue = [0, 0, 255, 255];
    assert_eq!(result.frames[0].pixels, red.repeat(4));
    assert_eq!(&result.frames[1].pixels[4..8], &green);
    assert_eq!(&result.frames[1].pixels[0..4], &red);
    assert_eq!(
        &result.frames[2].pixels[4..8],
        &red,
        "Previous restores the prior canvas"
    );
    assert_eq!(&result.frames[2].pixels[8..12], &blue);
    assert_eq!(result.frames[0].duration, Some(40_000));
    assert_eq!(result.frames[1].timestamp, 40_000);
    assert_eq!(result.frames[2].timestamp, 90_000);
}

#[test]
fn apng_composition_and_separate_poster_are_distinct_tracks() {
    for poster in [false, true] {
        let result = codecs::decode(
            "image/png",
            &test_fixtures::apng(poster),
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result.poster.is_some(), poster);
        assert_eq!(result.frames.len(), 3);
        assert_eq!(result.repetitions, Some(1));
        assert_eq!(&result.frames[1].pixels[4..8], &[0, 255, 0, 255]);
        assert_eq!(&result.frames[2].pixels[4..8], &[255, 0, 0, 255]);
        assert_eq!(&result.frames[2].pixels[8..12], &[0, 0, 255, 255]);
        assert_eq!(result.frames[2].timestamp, 80_000);
        if let Some(frame) = result.poster {
            assert_eq!(frame.pixels, [90, 80, 70, 255].repeat(4));
            assert_eq!(frame.duration, None);
        }
    }
}

#[test]
fn animated_webp_decodes_distinct_frames_and_encoded_play_count() {
    let result = codecs::decode(
        "image/webp",
        &test_fixtures::animated_webp(),
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.repetitions, Some(1));
    for (index, color) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .enumerate()
    {
        assert_eq!(result.frames[index].pixels, color.repeat(4));
        assert_eq!(result.frames[index].timestamp, index as u64 * 40_000);
    }
}

#[test]
fn every_owned_encoded_fixture_matches_its_declared_container() {
    for (name, mime, bytes) in test_fixtures::all() {
        let frames = codecs::decode(mime, &bytes, false, &AtomicBool::new(false)).unwrap();
        assert!(!frames.frames.is_empty(), "{name}");
        assert!(
            frames
                .frames
                .iter()
                .all(|frame| frame.pixels.len() == (frame.width * frame.height * 4) as usize)
        );
    }
}
