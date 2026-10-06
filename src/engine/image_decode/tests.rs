use super::*;
use image::{ImageBuffer, ImageFormat, Rgba};

fn encoded_png(width: u32, height: u32, pixels: Vec<u8>) -> Vec<u8> {
    let image = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels).unwrap();
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn shared_decoder_preserves_straight_alpha_and_page_conversion_is_exact() {
    let pixels = vec![200, 100, 50, 128, 11, 22, 33, 0];
    let bytes = encoded_png(2, 1, pixels.clone());
    let image = decode(&bytes, DecodeLimits::CANVAS, DecodeOptions::default()).unwrap();
    assert_eq!(image.rgba, pixels);
    let presentation = image.into_premultiplied_bgra();
    assert_eq!(&*presentation.bgra, &[25, 50, 100, 128, 0, 0, 0, 0]);
}

#[test]
fn product_budget_rejects_before_raster_allocation() {
    let bytes = encoded_png(2, 2, vec![255; 16]);
    let limits = DecodeLimits {
        pixels: 3,
        ..DecodeLimits::CANVAS
    };
    assert!(
        decode(&bytes, limits, DecodeOptions::default())
            .unwrap_err()
            .contains("dimensions")
    );
}

#[test]
fn presentation_conversion_matches_the_original_equation_for_every_byte_and_alpha() {
    let rgba: Vec<u8> = (0_u8..=255)
        .flat_map(|alpha| {
            (0_u8..=255).flat_map(move |value| [value, 255 - value, value.wrapping_mul(17), alpha])
        })
        .collect();
    let expected: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|pixel| {
            let alpha = u32::from(pixel[3]);
            let convert = |channel| ((u32::from(pixel[channel]) * alpha + 127) / 255) as u8;
            [convert(2), convert(1), convert(0), pixel[3]]
        })
        .collect();
    let image = RasterImage::new(256, 256, rgba, DecodeLimits::CANVAS).unwrap();
    assert_eq!(&*image.into_premultiplied_bgra().bgra, expected);
}

#[test]
fn every_orientation_round_trips_pixels_and_rectangular_dimensions() {
    let pixels: Vec<u8> = (1..=6).flat_map(|value| [value, 0, 0, 255]).collect();
    for value in 1..=8 {
        let image = RasterImage::new(3, 2, pixels.clone(), DecodeLimits::CANVAS).unwrap();
        let output = Orientation(value)
            .apply(image, DecodeLimits::CANVAS)
            .unwrap();
        let output = Orientation(value)
            .inverse()
            .apply(output, DecodeLimits::CANVAS)
            .unwrap();
        assert_eq!((output.width, output.height), (3, 2));
        assert_eq!(output.rgba, pixels, "orientation {value}");
    }
}

#[test]
fn empty_truncated_and_non_image_sources_fail_cleanly() {
    for bytes in [
        &b""[..],
        b"not an image",
        b"\xff\x0a",
        b"\0\0\0\x0cJXL \x0d\x0a\x87\x0a",
    ] {
        assert!(decode(bytes, DecodeLimits::CANVAS, DecodeOptions::default()).is_err());
    }
}
