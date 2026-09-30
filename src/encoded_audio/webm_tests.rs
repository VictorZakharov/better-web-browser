use super::*;
use base64::Engine as _;

fn fixture(encoded: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded.split_whitespace().collect::<String>())
        .unwrap()
}

fn tone() -> Vec<u8> {
    fixture(include_str!(
        "../../tests/fixtures/media/test-0.4s-tone.webm.base64"
    ))
}

fn admit(bytes: &[u8]) -> Result<(), String> {
    crate::encoded_audio::validate(bytes, crate::encoded_audio::Kind::VorbisWebm)
}

fn offset(bytes: &[u8], marker: &[u8]) -> usize {
    bytes
        .windows(marker.len())
        .position(|part| part == marker)
        .unwrap()
}

#[test]
fn owned_vorbis_webm_is_admitted_but_real_opus_and_mixed_video_are_not() {
    assert!(admit(&tone()).is_ok());
    for encoded in [
        include_str!("../../tests/fixtures/media/test-0.4s-opus.webm.base64"),
        include_str!("../../tests/fixtures/media/test-0.4s-mixed.webm.base64"),
    ] {
        assert!(admit(&fixture(encoded)).is_err());
    }
}

#[test]
fn finite_source_rejects_every_truncated_suffix_and_appended_junk() {
    let good = tone();
    for end in 0..good.len() {
        assert!(
            admit(&good[..end]).is_err(),
            "accepted truncated byte {end}"
        );
    }
    let mut trailing = good;
    trailing.extend_from_slice(&[0, 0]);
    assert!(admit(&trailing).is_err());
}

#[test]
fn supports_unknown_sized_segment_and_cluster_with_bounded_children() {
    let good = tone();
    let segment = offset(&good, &SEGMENT.to_be_bytes());
    let (_, width) = vint(&good[segment + 4..], false).unwrap();
    let mut unknown = good.clone();
    unknown[segment + 4] = 0xff >> (width - 1);
    unknown[segment + 5..segment + 4 + width].fill(255);
    assert!(admit(&unknown).is_ok());
    let cluster = offset(&unknown, &CLUSTER.to_be_bytes());
    let (_, width) = vint(&unknown[cluster + 4..], false).unwrap();
    unknown[cluster + 4] = 0xff >> (width - 1);
    unknown[cluster + 5..cluster + 4 + width].fill(255);
    assert!(admit(&unknown).is_ok());
    // The following finite Cues remains a Segment sibling, not cluster data.
    assert!(
        unknown
            .windows(4)
            .any(|part| part == [0x1c, 0x53, 0xbb, 0x6b])
    );
}

#[test]
fn rejects_wrong_doctype_hostile_header_lengths_and_unknown_leaf_size() {
    let good = tone();
    let mut wrong = good.clone();
    let doc = offset(&wrong, b"webm");
    wrong[doc..doc + 4].copy_from_slice(b"mkv ");
    assert!(admit(&wrong).is_err());
    let private = offset(&good, &[0x63, 0xa2]);
    let mut huge = good.clone();
    huge[private + 2] = 1; // Eight-byte size claiming an impossible allocation.
    huge[private + 3..private + 10].fill(0xfe);
    assert!(admit(&huge).is_err());
    let mut unknown = good;
    let (_, width) = vint(&unknown[private + 2..], false).unwrap();
    unknown[private + 2] = 0xff >> (width - 1);
    unknown[private + 3..private + 2 + width].fill(255);
    assert!(admit(&unknown).is_err());
}

#[test]
fn rejects_impossible_xiph_lacing_and_hostile_setup_codebooks() {
    for bytes in [&[2, 255][..], &[2, 250, 250, 1][..], &[1, 0, 0][..]] {
        assert!(crate::ogg_vorbis_headers::preflight_xiph_laced(bytes, 8).is_err());
    }
    let mut good = tone();
    let book = offset(&good, &[5, b'v', b'o', b'r', b'b', b'i', b's']);
    // First codebook's 24-bit entry count, after count/sync/dimensions.
    good[book + 13..book + 16].fill(255);
    assert!(admit(&good).is_err());
}

#[test]
fn variable_integer_bounds_cover_all_widths_and_zero() {
    assert!(vint(&[0], false).is_err());
    assert!(vint(&[1, 0, 0], false).is_err());
    assert!(vint(&[8, 0, 0, 0, 0], true).is_err());
    for width in 1..=8 {
        let mut bytes = vec![0; width];
        bytes[0] = 0x80 >> (width - 1);
        assert_eq!(vint(&bytes, false).unwrap(), (0, width));
    }
}

#[test]
fn missing_cluster_timestamp_cannot_silently_discard_supported_audio() {
    let mut source = tone();
    let cluster = offset(&source, &CLUSTER.to_be_bytes());
    let (_, width) = vint(&source[cluster + 4..], false).unwrap();
    let mut cursor = cluster + 4 + width;
    loop {
        let (id, id_width) = vint(&source[cursor..], true).unwrap();
        let (size, size_width) = vint(&source[cursor + id_width..], false).unwrap();
        if id == 0xe7 {
            source[cursor] = 0xec;
            break;
        } // Finite Void instead of Timestamp.
        cursor += id_width + size_width + size as usize;
    }
    assert!(admit(&source).is_err());
    let mut malformed = tone();
    malformed[cursor + 1] = 0x80; // Timestamp with zero unsigned width.
    assert!(admit(&malformed).is_err());
}
