use super::*;
use test_support::*;

fn transformed(properties: &[Vec<u8>], order: &[u16], ignore: bool) -> DecodeResult<RasterImage> {
    let bytes = metadata(
        1,
        properties,
        &[associations(0, false, &[(1, order.to_vec())])],
    );
    parse(&bytes)?.transform(image(), DecodeLimits::CANVAS, ignore)
}

#[test]
fn all_four_counterclockwise_rotations_map_rectangular_pixels() {
    let expected = [
        vec![1, 2, 3, 4, 5, 6],
        vec![3, 6, 2, 5, 1, 4],
        vec![6, 5, 4, 3, 2, 1],
        vec![4, 1, 5, 2, 6, 3],
    ];
    for (rotation, pixels) in expected.into_iter().enumerate() {
        let output = transformed(&[boxed(b"irot", &[rotation as u8])], &[0x81], false).unwrap();
        assert_eq!(red_pixels(&output), pixels);
        assert_eq!(
            (output.width, output.height),
            if rotation % 2 == 0 { (3, 2) } else { (2, 3) }
        );
    }
}

#[test]
fn mirror_axes_and_association_transform_order_are_not_interchangeable() {
    let horizontal = transformed(&[boxed(b"imir", &[0])], &[0x81], false).unwrap();
    assert_eq!(red_pixels(&horizontal), [3, 2, 1, 6, 5, 4]);
    let vertical = transformed(&[boxed(b"imir", &[1])], &[0x81], false).unwrap();
    assert_eq!(red_pixels(&vertical), [4, 5, 6, 1, 2, 3]);
    let properties = [boxed(b"irot", &[1]), boxed(b"imir", &[0])];
    let first = transformed(&properties, &[0x81, 0x82], false).unwrap();
    let second = transformed(&properties, &[0x82, 0x81], false).unwrap();
    assert_eq!(red_pixels(&first), [6, 3, 5, 2, 4, 1]);
    assert_eq!(red_pixels(&second), [1, 4, 2, 5, 3, 6]);
}

#[test]
fn ignore_orientation_does_not_ignore_clean_aperture() {
    let properties = [
        boxed(b"irot", &[1]),
        boxed(b"clap", &aperture((1, 1), (2, 1), (0, 1), (0, 1))),
    ];
    let output = transformed(&properties, &[0x81, 0x82], true).unwrap();
    assert_eq!((output.width, output.height), (1, 2));
    assert_eq!(red_pixels(&output), [2, 5]);
}

#[test]
fn clean_aperture_rationals_support_exact_dimensions_and_signed_offsets() {
    let properties = [boxed(b"clap", &aperture((2, 2), (4, 2), (-2, 2), (0, 1)))];
    let output = transformed(&properties, &[0x81], false).unwrap();
    assert_eq!(red_pixels(&output), [1, 4]);
    let properties = [boxed(b"clap", &aperture((1, 1), (2, 1), (1, 1), (0, 1)))];
    assert_eq!(
        red_pixels(&transformed(&properties, &[0x81], false).unwrap()),
        [3, 6]
    );
}

#[test]
fn invalid_apertures_reject_fractional_origin_outside_crop_and_zero_denominators() {
    let invalid = [
        aperture((1, 0), (2, 1), (0, 1), (0, 1)),
        aperture((1, 2), (2, 1), (0, 1), (0, 1)),
        aperture((2, 1), (2, 1), (0, 1), (0, 1)),
        aperture((4, 1), (2, 1), (0, 1), (0, 1)),
        aperture((1, 1), (2, 1), (-2, 1), (0, 1)),
        aperture((0, 1), (2, 1), (0, 1), (0, 1)),
        aperture((1, 1), (2, 1), (0, 1), (0, 0)),
    ];
    for payload in invalid {
        assert!(transformed(&[boxed(b"clap", &payload)], &[0x81], false).is_err());
    }
}

#[test]
fn extreme_clean_aperture_arithmetic_never_wraps_or_panics() {
    for numerator in [0, u32::MAX, 1] {
        for offset in [i32::MIN, i32::MAX, 0] {
            let payload = aperture((numerator, 1), (2, 1), (offset, u32::MAX), (0, 1));
            let _ = transformed(&[boxed(b"clap", &payload)], &[0x81], false);
        }
    }
}

#[test]
fn spatial_dimensions_must_match_the_decoded_image_before_transformations() {
    let valid = [spatial(3, 2), boxed(b"irot", &[1])];
    assert_eq!(transformed(&valid, &[0x81, 0x82], false).unwrap().width, 2);
    for (width, height) in [(2, 3), (0, 2), (3, 0), (u32::MAX, 2)] {
        assert!(transformed(&[spatial(width, height)], &[0x81], false).is_err());
    }
    assert!(transformed(&[spatial(3, 2), spatial(3, 2)], &[0x81, 0x82], false).is_err());
}

#[test]
fn cicp_and_icc_profiles_are_distinct_properties_with_separate_duplicate_checks() {
    let mut value = Metadata::default();
    let cicp = b"nclx\0\x01\0\x0d\0\x01\x80";
    apply_property(&mut value, *b"colr", cicp, true).unwrap();
    assert_eq!(
        value.cicp,
        Some(Cicp {
            primaries: 1,
            transfer: 13,
            matrix: 1,
            full_range: true
        })
    );
    apply_property(&mut value, *b"colr", b"profowned profile", true).unwrap();
    assert_eq!(value.icc, Some(&b"owned profile"[..]));
    assert!(apply_property(&mut value, *b"colr", cicp, true).is_err());
    assert!(apply_property(&mut value, *b"colr", b"rICCsecond", true).is_err());
}

#[test]
fn unknown_color_profiles_and_non_square_pixel_aspects_fail_explicitly() {
    assert!(apply_property(&mut Metadata::default(), *b"colr", b"zzzz", false).is_ok());
    assert!(apply_property(&mut Metadata::default(), *b"colr", b"zzzz", true).is_err());
    for (x, y) in [(1u32, 2u32), (0, 0), (1, 0)] {
        let mut payload = x.to_be_bytes().to_vec();
        payload.extend_from_slice(&y.to_be_bytes());
        assert!(apply_property(&mut Metadata::default(), *b"pasp", &payload, true).is_err());
    }
    assert!(
        apply_property(
            &mut Metadata::default(),
            *b"pasp",
            b"\0\0\0\x01\0\0\0\x01",
            true
        )
        .is_ok()
    );
}

#[test]
fn fixed_length_properties_reject_extra_or_truncated_fields() {
    for size in 0..11 {
        let payload = &b"nclx\0\x01\0\x0d\0\x01\x80"[..size];
        assert!(apply_property(&mut Metadata::default(), *b"colr", payload, true).is_err());
    }
    assert!(apply_property(&mut Metadata::default(), *b"irot", &[1, 0], true).is_err());
    assert!(apply_property(&mut Metadata::default(), *b"imir", &[], true).is_err());
    assert!(apply_property(&mut Metadata::default(), *b"ispe", &[0; 13], true).is_err());
}
