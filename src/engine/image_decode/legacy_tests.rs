//! Shared metadata handling must not regress existing image formats.
use super::*;
use image::{ColorType, ImageEncoder, codecs::jpeg::JpegEncoder};

fn jpeg() -> Vec<u8> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 100)
        .write_image(
            &[
                255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 128, 64, 32,
            ],
            3,
            2,
            ColorType::Rgb8.into(),
        )
        .unwrap();
    bytes
}

fn insert_segment(jpeg: &[u8], marker: u8, payload: &[u8]) -> Vec<u8> {
    let mut output = jpeg[..2].to_vec();
    output.extend_from_slice(&[0xff, marker]);
    output.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    output.extend_from_slice(payload);
    output.extend_from_slice(&jpeg[2..]);
    output
}

fn oriented_jpeg(orientation: u16, little_endian: bool) -> Vec<u8> {
    let mut tiff = Vec::new();
    if little_endian {
        tiff.extend_from_slice(b"II\x2a\0\x08\0\0\0\x01\0");
        tiff.extend_from_slice(b"\x12\x01\x03\0\x01\0\0\0");
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0; 6]);
    } else {
        tiff.extend_from_slice(b"MM\0\x2a\0\0\0\x08\0\x01");
        tiff.extend_from_slice(b"\x01\x12\0\x03\0\0\0\x01");
        tiff.extend_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&[0; 6]);
    }
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(tiff);
    insert_segment(&jpeg(), 0xe1, &payload)
}

#[test]
fn exif_orientation_uses_both_tiff_byte_orders_and_can_be_explicitly_ignored() {
    let baseline = decode(&jpeg(), DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    let mappings = [
        vec![0, 1, 2, 3, 4, 5],
        vec![2, 1, 0, 5, 4, 3],
        vec![5, 4, 3, 2, 1, 0],
        vec![3, 4, 5, 0, 1, 2],
        vec![0, 3, 1, 4, 2, 5],
        vec![3, 0, 4, 1, 5, 2],
        vec![5, 2, 4, 1, 3, 0],
        vec![2, 5, 1, 4, 0, 3],
    ];
    for little in [true, false] {
        for (index, mapping) in mappings.iter().enumerate() {
            let bytes = oriented_jpeg(index as u16 + 1, little);
            let actual = decode(&bytes, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
            let expected = mapping
                .iter()
                .flat_map(|&index| baseline.rgba[index * 4..index * 4 + 4].iter().copied())
                .collect::<Vec<_>>();
            assert_eq!(
                actual.rgba,
                expected,
                "orientation {} little={little}",
                index + 1
            );
            assert_eq!(
                (actual.width, actual.height),
                if index < 4 { (3, 2) } else { (2, 3) }
            );
            let ignored = decode(
                &bytes,
                DecodeLimits::CANVAS,
                DecodeOptions {
                    ignore_orientation: true,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(ignored.rgba, baseline.rgba);
            assert_eq!((ignored.width, ignored.height), (3, 2));
        }
    }
}

#[test]
fn jpeg_icc_profile_changes_real_pixels_but_none_preserves_encoded_rgb() {
    let raw = jpeg();
    let baseline = decode(&raw, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    let profile = moxcms::ColorProfile::new_display_p3().encode().unwrap();
    let mut payload = b"ICC_PROFILE\0\x01\x01".to_vec();
    payload.extend_from_slice(&profile);
    let encoded = insert_segment(&raw, 0xe2, &payload);
    let ignored = decode(
        &encoded,
        DecodeLimits::CANVAS,
        DecodeOptions {
            ignore_color_profile: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ignored.rgba, baseline.rgba);
    let converted = decode(&encoded, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    let mut expected = baseline;
    color::apply_icc(&mut expected, &profile).unwrap();
    assert_eq!(converted.rgba, expected.rgba);
    assert_ne!(converted.rgba, ignored.rgba);
}

#[test]
fn png_bmp_and_webp_keep_identical_straight_alpha_where_format_supports_it() {
    let source = [
        255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 0, 0, 0, 0, 0, 0, 0, 255, 128, 64, 32, 192,
    ];
    for format in [
        image::ImageFormat::Png,
        image::ImageFormat::Bmp,
        image::ImageFormat::WebP,
    ] {
        let image = image::RgbaImage::from_raw(3, 2, source.to_vec()).unwrap();
        let mut output = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut output, format)
            .unwrap();
        let actual = decode(
            &output.into_inner(),
            DecodeLimits::CANVAS,
            DecodeOptions::default(),
        )
        .unwrap();
        assert_eq!(actual.rgba, source, "{format:?}");
    }
}

#[test]
fn legacy_decoder_checks_pixel_product_not_just_each_axis() {
    let bytes = jpeg();
    let limits = DecodeLimits {
        dimension: 100,
        pixels: 5,
        ..DecodeLimits::CANVAS
    };
    assert!(
        decode(&bytes, limits, DecodeOptions::default())
            .unwrap_err()
            .contains("dimensions")
    );
    let limits = DecodeLimits {
        working_bytes: 23,
        ..DecodeLimits::CANVAS
    };
    assert!(decode(&bytes, limits, DecodeOptions::default()).is_err());
}

#[test]
fn metadata_choices_are_independent_orientation_does_not_disable_color_conversion() {
    let bytes = oriented_jpeg(6, true);
    let profile = moxcms::ColorProfile::new_display_p3().encode().unwrap();
    let mut payload = b"ICC_PROFILE\0\x01\x01".to_vec();
    payload.extend(profile);
    let bytes = insert_segment(&bytes, 0xe2, &payload);
    for ignore_orientation in [true, false] {
        for ignore_color_profile in [true, false] {
            let actual = decode(
                &bytes,
                DecodeLimits::CANVAS,
                DecodeOptions {
                    ignore_orientation,
                    ignore_color_profile,
                },
            )
            .unwrap();
            assert_eq!(
                (actual.width, actual.height),
                if ignore_orientation { (3, 2) } else { (2, 3) }
            );
        }
    }
}

#[test]
fn format_sniffing_rejects_unrelated_ftyp_brands_and_non_image_signatures() {
    let mut bytes = 16u32.to_be_bytes().to_vec();
    bytes.extend_from_slice(b"ftypheic\0\0\0\0");
    bytes.extend_from_slice(b"avif");
    assert!(!avif::matches(&bytes));
    assert!(decode(&bytes, DecodeLimits::CANVAS, DecodeOptions::default()).is_err());
    bytes[..4].copy_from_slice(&20u32.to_be_bytes());
    assert!(avif::matches(&bytes));
    assert!(decode(&bytes, DecodeLimits::CANVAS, DecodeOptions::default()).is_err());
}
