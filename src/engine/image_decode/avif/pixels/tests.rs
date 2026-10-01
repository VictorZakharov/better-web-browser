use super::*;
use crate::engine::image_decode::fixtures::find;

fn item(name: &str) -> Vec<u8> {
    avif_parse::read_avif(&mut std::io::Cursor::new(find(name)))
        .unwrap()
        .primary_item
        .to_vec()
}

#[test]
fn alpha_auxiliary_uses_full_range_without_color_transforming_coverage() {
    for name in ["gray.avif", "gray10.avif"] {
        let picture = super::super::decoder::decode(&item(name), DecodeLimits::CANVAS).unwrap();
        let mut output =
            RasterImage::new(3, 2, [10, 20, 30, 255].repeat(6), DecodeLimits::CANVAS).unwrap();
        apply_alpha(&mut output, &picture, DecodeLimits::CANVAS).unwrap();
        let expected = if name == "gray.avif" {
            [76, 150, 29, 255, 0, 79]
        } else {
            // Independent FFmpeg gray10le readback is [305, 599, 116,
            // 1020, 0, 318]. Full-range alpha maps 1020/1023 to 254.
            [76, 149, 29, 254, 0, 79]
        };
        assert_eq!(
            output
                .rgba
                .chunks_exact(4)
                .map(|pixel| pixel[3])
                .collect::<Vec<_>>(),
            expected
        );
        for pixel in output.rgba.chunks_exact(4) {
            assert_eq!(&pixel[..3], &[10, 20, 30]);
        }
    }
}

#[test]
fn alpha_plane_must_match_color_geometry_and_be_monochrome() {
    let color =
        super::super::decoder::decode(&item("rgb-lossless.avif"), DecodeLimits::CANVAS).unwrap();
    let mut output =
        RasterImage::new(3, 2, [1, 2, 3, 255].repeat(6), DecodeLimits::CANVAS).unwrap();
    assert!(
        apply_alpha(&mut output, &color, DecodeLimits::CANVAS)
            .unwrap_err()
            .contains("monochrome")
    );
    let alpha = super::super::decoder::decode(&item("gray.avif"), DecodeLimits::CANVAS).unwrap();
    let mut wrong = RasterImage::new(2, 3, [1, 2, 3, 255].repeat(6), DecodeLimits::CANVAS).unwrap();
    assert!(
        apply_alpha(&mut wrong, &alpha, DecodeLimits::CANVAS)
            .unwrap_err()
            .contains("dimensions")
    );
    let limits = DecodeLimits {
        pixels: 5,
        ..DecodeLimits::CANVAS
    };
    assert!(apply_alpha(&mut output, &alpha, limits).is_err());
}

#[test]
fn decode_options_do_not_change_auxiliary_alpha_coverage() {
    let bytes = find("rgba-lossless.avif");
    for ignore_orientation in [false, true] {
        for ignore_color_profile in [false, true] {
            let output = crate::engine::image_decode::decode(
                &bytes,
                DecodeLimits::CANVAS,
                crate::engine::image_decode::DecodeOptions {
                    ignore_orientation,
                    ignore_color_profile,
                },
            )
            .unwrap();
            assert_eq!(
                output
                    .rgba
                    .chunks_exact(4)
                    .map(|pixel| pixel[3])
                    .collect::<Vec<_>>(),
                [255, 128, 64, 0, 255, 192]
            );
        }
    }
}

#[test]
fn supported_matrix_set_is_explicit_and_invalid_coefficients_are_not_guessed() {
    for value in [1, 2, 4, 5, 6, 7, 9] {
        assert!(matrix(value).is_ok(), "{value}");
    }
    for value in [0, 3, 8, 10, 11, 12, 13, 14, u16::MAX] {
        assert!(matrix(value).is_err(), "{value}");
    }
}

#[test]
fn ten_and_twelve_bit_extreme_channels_saturate_without_wrapping_to_dark_colors() {
    for depth in [8, 10, 12] {
        for full in [true, false] {
            assert_eq!(sample(0, depth, full), 0);
            assert_eq!(sample(u16::MAX as u32, depth, full), 255);
            assert_eq!(sample(u32::MAX, depth, full), 255);
        }
    }
    let output = crate::engine::image_decode::decode(
        &find("yuv444-10.avif"),
        DecodeLimits::CANVAS,
        crate::engine::image_decode::DecodeOptions::default(),
    )
    .unwrap();
    assert_eq!(
        output.rgba[0], 255,
        "scalar YUV saturation regression: red must not wrap to 2"
    );
}

#[test]
fn full_and_limited_sample_ranges_include_both_endpoints() {
    for depth in [8, 10, 12] {
        assert_eq!(sample(0, depth, true), 0);
        assert_eq!(sample((1 << depth) - 1, depth, true), 255);
        assert_eq!(sample(16 << (depth - 8), depth, false), 0);
        assert_eq!(sample(235 << (depth - 8), depth, false), 255);
    }
}

#[test]
fn identity_matrix_is_gbr_not_yuv_or_rgb_plane_order() {
    let planes = Planes {
        width: 1,
        height: 1,
        chroma_width: 1,
        y: vec![64u8],
        u: vec![32],
        v: vec![128],
    };
    let mut pixel = [0; 4];
    identity(&planes, &mut pixel, 8, true).unwrap();
    assert_eq!(pixel, [128, 64, 32, 255]);
}
