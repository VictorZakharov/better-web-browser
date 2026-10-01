use super::*;
use crate::engine::image_decode::DecodeLimits;

fn image(pixels: &[u8]) -> RasterImage {
    RasterImage::new(
        (pixels.len() / 4) as u32,
        1,
        pixels.to_vec(),
        DecodeLimits::CANVAS,
    )
    .unwrap()
}

#[test]
fn srgb_profile_roundtrip_preserves_coverage_and_rgb_with_small_cms_rounding() {
    let profile = ColorProfile::new_srgb().encode().unwrap();
    let pixels = [12, 34, 56, 0, 123, 45, 67, 128, 255, 255, 255, 255];
    let mut actual = image(&pixels);
    apply_icc(&mut actual, &profile).unwrap();
    for (index, (&actual, &expected)) in actual.rgba.iter().zip(&pixels).enumerate() {
        assert!(
            actual.abs_diff(expected) <= if index % 4 == 3 { 0 } else { 1 },
            "byte {index}: {actual} != {expected}"
        );
    }
}

#[test]
fn grayscale_gamma_is_converted_to_rgb_without_changing_alpha() {
    let profile = ColorProfile::new_gray_with_gamma(1.0);
    let mut actual = image(&[128, 128, 128, 37]);
    apply_profile(&mut actual, &profile).unwrap();
    assert!((186..=190).contains(&actual.rgba[0]), "{:?}", actual.rgba);
    assert_eq!(actual.rgba[0], actual.rgba[1]);
    assert_eq!(actual.rgba[1], actual.rgba[2]);
    assert_eq!(actual.rgba[3], 37);
}

#[test]
fn malformed_and_oversized_profiles_are_rejected_instead_of_assumed_srgb() {
    for bytes in [&[][..], b"ICC profile", &[0; 128]] {
        assert!(apply_icc(&mut image(&[1, 2, 3, 255]), bytes).is_err());
    }
    let oversized = vec![0; MAX_PROFILE_BYTES + 1];
    assert!(
        apply_icc(&mut image(&[1, 2, 3, 255]), &oversized)
            .unwrap_err()
            .contains("budget")
    );
}

#[test]
fn associated_alpha_is_recovered_once_with_saturation_and_zero_coverage_normalization() {
    let mut actual = image(&[128, 64, 32, 128, 255, 128, 64, 0, 200, 200, 200, 100]);
    unpremultiply(&mut actual);
    assert_eq!(
        actual.rgba,
        [255, 128, 64, 128, 0, 0, 0, 0, 255, 255, 255, 100]
    );
}

#[test]
fn row_scratch_handles_a_rectangular_image_without_touching_the_next_rows_alpha() {
    let mut actual = RasterImage::new(
        2,
        2,
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16].to_vec(),
        DecodeLimits::CANVAS,
    )
    .unwrap();
    apply_profile(&mut actual, &ColorProfile::new_display_p3()).unwrap();
    assert_eq!(
        actual
            .rgba
            .chunks_exact(4)
            .map(|pixel| pixel[3])
            .collect::<Vec<_>>(),
        [4, 8, 12, 16]
    );
    assert_eq!(actual.rgba.len(), 16);
}
