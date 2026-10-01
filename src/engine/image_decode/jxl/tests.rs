use super::super::fixtures::find;
use super::*;

fn box_bytes(kind: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
    let mut output = ((bytes.len() + 8) as u32).to_be_bytes().to_vec();
    output.extend_from_slice(kind);
    output.extend_from_slice(bytes);
    output
}

fn container(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut output = b"\0\0\0\x0cJXL \x0d\x0a\x87\x0a".to_vec();
    output.extend(box_bytes(b"ftyp", b"jxl \0\0\0\0jxl "));
    for part in parts {
        output.extend_from_slice(part);
    }
    output
}

#[test]
fn bare_and_container_codestreams_produce_identical_pixels() {
    let raw = find("rgba-lossless.jxl");
    let wrapped = container(&[box_bytes(b"jxlc", &raw)]);
    let expected = decode(&raw, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    let actual = decode(&wrapped, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    assert_eq!(actual.rgba, expected.rgba);
    assert_eq!(
        (actual.width, actual.height),
        (expected.width, expected.height)
    );
}

#[test]
fn partial_container_segments_are_reassembled_in_order() {
    let raw = find("rgb-lossless.jxl");
    for split in [2, 3, raw.len() / 2, raw.len() - 1] {
        let mut first = 0u32.to_be_bytes().to_vec();
        first.extend_from_slice(&raw[..split]);
        let mut second = 0x80000001u32.to_be_bytes().to_vec();
        second.extend_from_slice(&raw[split..]);
        let wrapped = container(&[box_bytes(b"jxlp", &first), box_bytes(b"jxlp", &second)]);
        let image = decode(&wrapped, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
        assert_eq!(
            image.rgba,
            super::super::fixtures::bytes(super::super::fixtures::MODERN[7].reference)
        );
    }
}

#[test]
fn auxiliary_metadata_is_not_decompressed_to_paint_an_image() {
    let raw = find("rgb-lossless.jxl");
    let expected = decode(&raw, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    // Deliberately invalid compressed XML: it must be ignored as an auxiliary
    // box, never sent to a Brotli decoder that can expand without the grid cap.
    let compressed = box_bytes(b"brob", b"xml not a Brotli stream");
    let wrapped = container(&[
        compressed,
        box_bytes(b"jxlc", &raw),
        box_bytes(b"Exif", &[0; 16]),
    ]);
    let actual = decode(&wrapped, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    assert_eq!(actual.rgba, expected.rgba);
}

#[test]
fn partial_sequences_reject_missing_final_index_reordering_and_duplicates() {
    let raw = find("rgb-lossless.jxl");
    let mut part = 0u32.to_be_bytes().to_vec();
    part.extend_from_slice(&raw);
    assert!(
        decode(
            &container(&[box_bytes(b"jxlp", &part)]),
            DecodeLimits::CANVAS,
            DecodeOptions::default()
        )
        .is_err()
    );
    part[..4].copy_from_slice(&0x80000001u32.to_be_bytes());
    assert!(
        decode(
            &container(&[box_bytes(b"jxlp", &part)]),
            DecodeLimits::CANVAS,
            DecodeOptions::default()
        )
        .is_err()
    );
    part[..4].copy_from_slice(&0x80000000u32.to_be_bytes());
    assert!(
        decode(
            &container(&[box_bytes(b"jxlp", &part), box_bytes(b"jxlp", &part)]),
            DecodeLimits::CANVAS,
            DecodeOptions::default()
        )
        .is_err()
    );
}

#[test]
fn truncated_container_payloads_cannot_be_treated_as_complete_images() {
    let raw = find("rgb-lossless.jxl");
    let wrapped = container(&[box_bytes(b"jxlc", &raw)]);
    for size in 0..wrapped.len() {
        assert!(
            decode(
                &wrapped[..size],
                DecodeLimits::CANVAS,
                DecodeOptions::default()
            )
            .is_err(),
            "prefix {size}"
        );
    }
}

#[test]
fn header_dimensions_and_grid_allocations_have_separate_caps() {
    let bytes = find("rgb-lossless.jxl");
    for limits in [
        DecodeLimits {
            pixels: 5,
            ..DecodeLimits::CANVAS
        },
        DecodeLimits {
            dimension: 2,
            ..DecodeLimits::CANVAS
        },
        DecodeLimits {
            working_bytes: 23,
            ..DecodeLimits::CANVAS
        },
    ] {
        assert!(decode(&bytes, limits, DecodeOptions::default()).is_err());
    }
    let limits = DecodeLimits {
        working_bytes: 24,
        ..DecodeLimits::CANVAS
    };
    // Six output pixels fit, but codec intermediate grids do not.
    assert!(decode(&bytes, limits, DecodeOptions::default()).is_err());
}

#[test]
fn header_sniffing_does_not_claim_jpeg_or_arbitrary_boxed_files() {
    for bytes in [
        &b"\xff\xd8"[..],
        b"\0\0\0\x0cJXL \0\0\0\0",
        b"jxl ",
        b"\xff",
    ] {
        assert!(!matches(bytes));
    }
    assert!(matches(&find("rgb-lossless.jxl")));
}
