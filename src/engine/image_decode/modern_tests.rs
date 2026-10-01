use super::*;
use fixtures::{MODERN, bytes, find};

#[test]
fn modern_codecs_decode_real_pixels_against_independent_native_references() {
    for fixture in MODERN {
        // FFmpeg's 12-bit 4:2:0 path interpolates chroma, unlike Chromium's
        // nearest reconstruction. Check that format against the browser below.
        if fixture.name == "yuv420-12.avif" {
            continue;
        }
        let encoded = bytes(fixture.encoded);
        let image = decode(&encoded, DecodeLimits::CANVAS, DecodeOptions::default())
            .unwrap_or_else(|error| panic!("{}: {error}", fixture.name));
        assert_eq!((image.width, image.height), (3, 2), "{}", fixture.name);
        let reference = bytes(fixture.reference);
        for (index, (&actual, &expected)) in image.rgba.iter().zip(&reference).enumerate() {
            assert!(
                actual.abs_diff(expected) <= fixture.tolerance,
                "{} byte {index}: {actual} != {expected}; actual {:?}, expected {:?}",
                fixture.name,
                image.rgba,
                reference
            );
        }
    }
}

#[test]
fn twelve_bit_avif_matches_chromium_canvas_readback() {
    // Chrome 154.0.8037.92, unified headless, createImageBitmap + getImageData.
    // Captured from the project-owned probe, not copied third-party imagery.
    let expected = [
        81, 81, 81, 255, 144, 144, 144, 255, 65, 36, 19, 255, 234, 234, 234, 255, 16, 16, 16, 255,
        108, 79, 62, 255,
    ];
    let image = decode(
        &find("yuv420-12.avif"),
        DecodeLimits::CANVAS,
        DecodeOptions::default(),
    )
    .unwrap();
    assert_eq!(image.rgba, expected);
}

#[test]
fn modern_images_reject_pixel_and_source_limits_without_panicking() {
    for fixture in MODERN {
        let encoded = bytes(fixture.encoded);
        let limits = DecodeLimits {
            pixels: 5,
            ..DecodeLimits::CANVAS
        };
        assert!(
            decode(&encoded, limits, DecodeOptions::default()).is_err(),
            "{}",
            fixture.name
        );
        let limits = DecodeLimits {
            source_bytes: encoded.len() - 1,
            ..DecodeLimits::CANVAS
        };
        assert!(
            decode(&encoded, limits, DecodeOptions::default()).is_err(),
            "{}",
            fixture.name
        );
        for size in 0..16.min(encoded.len()) {
            assert!(
                decode(
                    &encoded[..size],
                    DecodeLimits::CANVAS,
                    DecodeOptions::default()
                )
                .is_err(),
                "{} prefix {size}",
                fixture.name
            );
        }
    }
}

#[test]
fn page_presentation_and_canvas_share_modern_decoder_pixels() {
    for name in ["rgb-lossless.avif", "rgba-lossless.jxl"] {
        let encoded = find(name);
        let canvas = decode(&encoded, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
        let image = decode(&encoded, DecodeLimits::PAGE, DecodeOptions::default()).unwrap();
        assert_eq!(canvas.rgba, image.rgba);
        let pixels = image.into_premultiplied_bgra();
        for (straight, bgra) in canvas.rgba.chunks_exact(4).zip(pixels.bgra.chunks_exact(4)) {
            let alpha = u16::from(straight[3]);
            assert_eq!(
                bgra,
                &[
                    ((u16::from(straight[2]) * alpha + 127) / 255) as u8,
                    ((u16::from(straight[1]) * alpha + 127) / 255) as u8,
                    ((u16::from(straight[0]) * alpha + 127) / 255) as u8,
                    straight[3],
                ]
            );
        }
    }
}
