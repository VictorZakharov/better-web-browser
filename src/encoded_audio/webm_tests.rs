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

// Rebuild only EBML envelopes around our owned packets. This is not a codec
// fixture mutation: the final Cluster still carries a genuine Vorbis packet.
fn element(id: u32, payload: &[u8]) -> Vec<u8> {
    let id = id.to_be_bytes();
    let start = id.iter().position(|byte| *byte != 0).unwrap();
    let mut result = id[start..].to_vec();
    let size = (payload.len() as u64 | (1_u64 << 56)).to_be_bytes();
    result.extend_from_slice(&size);
    result.extend_from_slice(payload);
    result
}

fn with_child(mut bytes: &[u8], parent: u32, child: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let (id, id_width) = vint(bytes, true).unwrap();
        let id = id as u32;
        let (size, size_width) = vint(&bytes[id_width..], false).unwrap();
        let end = id_width + size_width + size as usize;
        let payload = &bytes[id_width + size_width..end];
        if id == parent {
            let mut payload = payload.to_vec();
            payload.extend_from_slice(child);
            result.extend_from_slice(&element(id, &payload));
        } else if master(id) {
            result.extend_from_slice(&element(id, &with_child(payload, parent, child)));
        } else {
            result.extend_from_slice(&bytes[..end]);
        }
        bytes = &bytes[end..];
    }
    result
}

fn first_block(bytes: &[u8]) -> Vec<u8> {
    let start = offset(bytes, &CLUSTER.to_be_bytes());
    let (_, width) = vint(&bytes[start + 4..], false).unwrap();
    let mut cursor = start + 4 + width;
    loop {
        let (id, id_width) = vint(&bytes[cursor..], true).unwrap();
        let (size, size_width) = vint(&bytes[cursor + id_width..], false).unwrap();
        let end = cursor + id_width + size_width + size as usize;
        if id == 0xa3 {
            return bytes[cursor + id_width + size_width..end].to_vec();
        }
        cursor = end;
    }
}

pub(crate) fn overflowing_second_cluster(good: &[u8]) -> Vec<u8> {
    appended_cluster(good, u64::MAX, &first_block(good))
}

fn appended_cluster(good: &[u8], timestamp: u64, block: &[u8]) -> Vec<u8> {
    // Preserve every original inner byte and SeekHead offset. Only mark the
    // existing Segment unknown-sized at the same width, then append a Cluster.
    let mut source = good.to_vec();
    let segment = offset(good, &SEGMENT.to_be_bytes());
    let (_, width) = vint(&good[segment + 4..], false).unwrap();
    source[segment + 4] = 0xff >> (width - 1);
    source[segment + 5..segment + 4 + width].fill(255);
    let mut cluster = element(0xe7, &timestamp.to_be_bytes());
    cluster.extend_from_slice(&element(0xa3, block));
    source.extend_from_slice(&element(CLUSTER, &cluster));
    source
}

#[test]
fn overflowing_second_cluster_cannot_be_reported_as_clean_eof_after_real_pcm() {
    let good = tone();
    let block = first_block(&good);
    for timestamp in [i64::MAX as u64 + 1, u64::MAX] {
        let source = appended_cluster(&good, timestamp, &block);
        let error = admit(&source).unwrap_err();
        assert!(error.contains("Timestamp overflows"), "{error}");
    }
    assert!(admit(&appended_cluster(&good, 1_000, &block)).is_ok());
}

#[test]
fn scaled_cluster_and_laced_duration_products_are_checked_before_probe() {
    let good = tone();
    for scale in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
        let source = with_child(&good, TRACK, &element(0x23314f, &scale.to_be_bytes()));
        assert!(admit(&source).is_err());
    }
    let scaled = with_child(&good, TRACK, &element(0x23314f, &2.0_f64.to_be_bytes()));
    assert!(admit(&scaled).is_ok());
    let defaulted = with_child(&good, TRACK, &element(0x23314f, &[]));
    assert!(admit(&defaulted).is_ok());
    let mut cluster = element(0xe7, &(i64::MAX as u64 / 2 + 1).to_be_bytes());
    cluster.extend_from_slice(&element(0xa3, &first_block(&good)));
    let source = with_child(&scaled, SEGMENT, &element(CLUSTER, &cluster));
    assert!(
        admit(&source)
            .unwrap_err()
            .contains("scaled Cluster Timestamp")
    );

    let mut laced = first_block(&good);
    let (_, width) = vint(&laced, false).unwrap();
    laced[width + 2] = (laced[width + 2] & !6) | 4; // Fixed lacing.
    laced.insert(width + 3, 1); // Two frames; actual lace parsing stays upstream.
    let mut cluster = element(0xe7, &1_000_u64.to_be_bytes());
    cluster.extend_from_slice(&element(0xa3, &laced));
    let source = with_child(&good, SEGMENT, &element(CLUSTER, &cluster));
    let source = with_child(&source, TRACK, &element(0x23e383, &u64::MAX.to_be_bytes()));
    assert!(
        admit(&source)
            .unwrap_err()
            .contains("DefaultDuration overflows")
    );

    let mut group = element(0xa1, &first_block(&good));
    group.extend_from_slice(&element(0x9b, &u64::MAX.to_be_bytes()));
    let mut cluster = element(0xe7, &1_000_u64.to_be_bytes());
    cluster.extend_from_slice(&element(0xa0, &group));
    let source = with_child(&good, SEGMENT, &element(CLUSTER, &cluster));
    assert!(admit(&source).is_err());
}

#[test]
fn fixed_block_relative_offset_and_codec_delay_cannot_hide_overflow() {
    let good = tone();
    let mut block = first_block(&good);
    let (_, width) = vint(&block, false).unwrap();
    block[width..width + 2].copy_from_slice(&1_i16.to_be_bytes());
    let mut cluster = element(0xe7, &(i64::MAX as u64).to_be_bytes());
    cluster.extend_from_slice(&element(0xa3, &block));
    let source = with_child(&good, SEGMENT, &element(CLUSTER, &cluster));
    // A large delay could rescue the final mathematical sum, but not the
    // upstream reader's intermediate signed addition before subtraction.
    let source = with_child(
        &source,
        TRACK,
        &element(0x56aa, &1_000_000_u64.to_be_bytes()),
    );
    assert!(
        admit(&source)
            .unwrap_err()
            .contains("relative Block timestamp")
    );
    let mut timing = timing::Timing::default();
    timing.metadata(0x2ad7b1, 0x1549_a966, &[1]).unwrap();
    timing
        .metadata(0x56aa, TRACK, &u64::MAX.to_be_bytes())
        .unwrap();
    timing.block(&[0x81, 0xff, 0xff, 0, 1]).unwrap();
    assert!(timing.validate().is_err());
}

#[test]
fn repeated_or_misplaced_info_cannot_change_the_preflight_timebase() {
    let good = tone();
    let info = element(0x1549_a966, &element(0x2ad7b1, &1_u64.to_be_bytes()));
    assert!(admit(&with_child(&good, SEGMENT, &info)).is_err());
    assert!(admit(&with_child(&good, TRACK, &info)).is_err());
}
