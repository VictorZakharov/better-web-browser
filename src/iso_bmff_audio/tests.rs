use super::*;
use base64::Engine;

fn fixture() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../tests/fixtures/media/test-0.4s-tone.m4a.base64")
                .lines()
                .collect::<String>(),
        )
        .unwrap()
}

#[test]
fn maps_aac_encoder_delay_to_exact_playable_frames() {
    let edit = ordinary_audio_edit(&fixture()).unwrap().unwrap();
    assert_eq!(
        edit.frame_window(44_100).unwrap(),
        FrameWindow {
            start: 1_024,
            end: 18_664
        }
    );
    assert!(edit.frame_window(44_101).is_err());
}

#[test]
fn metadata_bytes_named_mvex_do_not_make_an_ordinary_movie_fragmented() {
    let mut bytes = fixture();
    let label = bytes
        .windows(4)
        .position(|window| window == b"Lavf")
        .expect("self-authored fixture has an encoder metadata string");
    bytes[label..label + 4].copy_from_slice(b"mvex");
    assert!(ordinary_audio_edit(&bytes).unwrap().is_some());
}

#[test]
fn unsupported_edit_shapes_and_malformed_boxes_are_rejected() {
    let bytes = fixture();
    let marker = bytes
        .windows(4)
        .position(|window| window == b"elst")
        .unwrap();

    let mut multiple_edits = bytes.clone();
    multiple_edits[marker + 8..marker + 12].copy_from_slice(&2_u32.to_be_bytes());
    assert!(ordinary_audio_edit(&multiple_edits).is_err());

    let mut empty_edit = bytes.clone();
    empty_edit[marker + 16..marker + 20].copy_from_slice(&(-1_i32).to_be_bytes());
    assert!(ordinary_audio_edit(&empty_edit).is_err());

    let mut rate_change = bytes.clone();
    rate_change[marker + 20..marker + 24].copy_from_slice(&[0, 2, 0, 0]);
    assert!(ordinary_audio_edit(&rate_change).is_err());

    let mut fractional = bytes.clone();
    fractional[marker + 12..marker + 16].copy_from_slice(&401_u32.to_be_bytes());
    assert!(
        ordinary_audio_edit(&fractional)
            .unwrap()
            .unwrap()
            .frame_window(44_100)
            .is_err()
    );

    let mut malformed = bytes;
    malformed[0..4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(ordinary_audio_edit(&malformed).is_err());
}
