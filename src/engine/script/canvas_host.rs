//! Bounded serialization of the Canvas-owned sRGB bitmap via the existing image codecs.

use super::binding_helpers::argument_id;
use super::*;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use std::io::Cursor;

mod composite;
mod composite_layer;
mod coverage;
mod curves;
pub(in crate::engine::script) mod element_images;
mod fill;
mod glyph_paint;
mod gradient_mask;
mod image_paint;
mod mask_cache;
mod path;
mod pattern_mask;
mod shader_mask;
mod shadow;
mod solid_mask;
mod solid_path;
mod source_layer;
mod stroke_outline;
mod svg_path;
#[cfg(windows)]
pub(in crate::engine::script) mod text;
pub(crate) mod webgl;

use crate::limits::MAX_CANVAS_PIXELS;
const MAX_ENCODED_BYTES: usize = 24 * 1024 * 1024;

pub(super) fn canvas_host_call(operation: &str, args: &[JsValue]) -> JsResult<Option<JsValue>> {
    if operation == "canvasTextEnvironment" {
        // A worker has no element or root style. CSS's non-element root size is initial.
        return Ok(Some(JsValue::Array(vec![
            JsValue::from(300),
            JsValue::from(150),
            JsValue::from(16),
            JsValue::from("ltr".to_owned()),
            JsValue::from(String::new()),
        ])));
    }
    if operation == "canvasSvgPathSegments" {
        return Ok(Some(svg_path::segments(args)));
    }
    if operation == "canvasPaintSourceLayer" {
        return Ok(Some(source_layer::paint(args)));
    }
    if operation == "canvasPaintImage" {
        return Ok(Some(image_paint::paint(args)));
    }
    if operation == "canvasPaintGlyphs" {
        return Ok(Some(glyph_paint::paint(args)));
    }
    if operation == "canvasCompositeLayer" {
        return Ok(Some(composite_layer::paint(args)));
    }
    if operation == "canvasPaintGradientMask" {
        return Ok(Some(gradient_mask::paint(args)));
    }
    if operation == "canvasPaintPatternMask" {
        return Ok(Some(pattern_mask::paint(args)));
    }
    if operation == "canvasPaintSolidMask" {
        return Ok(Some(solid_mask::paint(args)));
    }
    if operation == "canvasPaintSolidPath" {
        return Ok(Some(solid_path::paint(args)));
    }
    if operation == "canvasCurvePoints" {
        return Ok(Some(curves::points(args)));
    }
    if operation == "canvasStrokeMask" {
        return Ok(Some(path::stroke_mask(args)));
    }
    if operation == "canvasFillMask" {
        return Ok(Some(fill::mask(args)));
    }
    if operation == "canvasStrokeContains" {
        return Ok(Some(path::stroke_contains(args)));
    }
    if operation == "canvasShadowLayer" {
        return Ok(Some(shadow::render(args)));
    }
    if operation == "canvasTextAvailable" {
        return Ok(Some(JsValue::Boolean(cfg!(windows))));
    }
    #[cfg(windows)]
    if let Some(value) = text::dispatch(operation, args)? {
        return Ok(Some(value));
    }
    if operation == "canvasDecode" {
        let Some(bytes) = args.get(1).and_then(JsValue::as_bytes) else {
            return Err(JsNativeError::typ()
                .with_message("ImageBitmap decoding requires an image byte array")
                .into());
        };
        let options = crate::engine::image_decode::DecodeOptions {
            ignore_orientation: matches!(args.get(2), Some(JsValue::Boolean(true))),
            ignore_color_profile: matches!(args.get(3), Some(JsValue::Boolean(true))),
        };
        let precise = matches!(args.get(4), Some(JsValue::Boolean(true)));
        return Ok(Some(match decode_raster(bytes, options, precise) {
            Some(image) => JsValue::Array(vec![
                JsValue::from(f64::from(image.width)),
                JsValue::from(f64::from(image.height)),
                JsValue::Bytes(image.rgba),
                image
                    .rgba16
                    .map(|words| {
                        JsValue::Bytes(words.into_iter().flat_map(u16::to_le_bytes).collect())
                    })
                    .unwrap_or(JsValue::Null),
            ]),
            None => JsValue::Null,
        }));
    }
    if operation != "canvasEncode" {
        return Ok(None);
    }
    let width = argument_id(args, 1);
    let height = argument_id(args, 2);
    let Some(pixel_count) = (width as usize).checked_mul(height as usize) else {
        return Ok(Some(JsValue::Null));
    };
    if pixel_count == 0 || pixel_count > MAX_CANVAS_PIXELS {
        return Ok(Some(JsValue::Null));
    }
    let Some(pixels) = args.get(5).and_then(JsValue::as_bytes) else {
        return Err(JsNativeError::typ()
            .with_message("Canvas encoding requires an RGBA byte array")
            .into());
    };
    if pixels.len() != pixel_count * 4 {
        return Err(JsNativeError::typ()
            .with_message("Canvas RGBA dimensions do not match the bitmap")
            .into());
    }
    let requested_type = args.get(3).map(JsValue::string_value).unwrap_or_default();
    let mime = match requested_type.to_ascii_lowercase().as_str() {
        "image/jpeg" => "image/jpeg",
        "image/webp" => "image/webp",
        _ => "image/png",
    };
    let quality = args.get(4).and_then(JsValue::as_number);
    let Some(bytes) = encode(width, height, pixels, mime, quality) else {
        return Ok(Some(JsValue::Null));
    };
    Ok(Some(JsValue::Array(vec![
        JsValue::from(mime.to_string()),
        JsValue::Bytes(bytes),
    ])))
}

#[cfg(test)]
fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    decode_with_options(bytes, Default::default())
}

#[cfg(test)]
fn decode_with_options(
    bytes: &[u8],
    options: crate::engine::image_decode::DecodeOptions,
) -> Option<(u32, u32, Vec<u8>)> {
    decode_raster(bytes, options, false).map(|image| (image.width, image.height, image.rgba))
}

fn decode_raster(
    bytes: &[u8],
    options: crate::engine::image_decode::DecodeOptions,
    precise: bool,
) -> Option<crate::engine::image_decode::RasterImage> {
    if bytes.is_empty() || bytes.len() > MAX_ENCODED_BYTES {
        return None;
    }
    if crate::engine::page::looks_like_svg_image(bytes) {
        let image = crate::engine::page::decode_svg_image_with_limits(
            bytes,
            "Canvas SVG",
            crate::engine::image_decode::DecodeLimits::CANVAS,
        )
        .ok()?;
        let mut rgba = image.bgra.to_vec();
        for pixel in rgba.chunks_exact_mut(4) {
            let alpha = u32::from(pixel[3]);
            if alpha == 0 {
                pixel[..3].fill(0);
            } else {
                let blue = u32::from(pixel[0]);
                let green = u32::from(pixel[1]);
                let red = u32::from(pixel[2]);
                let unpremultiply = |channel: u32| {
                    (channel * 255 + alpha / 2)
                        .checked_div(alpha)
                        .unwrap_or(0)
                        .min(255) as u8
                };
                pixel[0] = unpremultiply(red);
                pixel[1] = unpremultiply(green);
                pixel[2] = unpremultiply(blue);
            }
        }
        return crate::engine::image_decode::RasterImage::new(
            image.width,
            image.height,
            rgba,
            crate::engine::image_decode::DecodeLimits::CANVAS,
        )
        .ok();
    }
    let decode = if precise {
        crate::engine::image_decode::decode_precise
    } else {
        crate::engine::image_decode::decode
    };
    decode(
        bytes,
        crate::engine::image_decode::DecodeLimits::CANVAS,
        options,
    )
    .ok()
}

fn encode(
    width: u32,
    height: u32,
    pixels: &[u8],
    mime: &str,
    quality: Option<f64>,
) -> Option<Vec<u8>> {
    let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, pixels.to_vec())?;
    let mut output = Vec::new();
    let result = match mime {
        "image/jpeg" => {
            let quality = quality
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                .unwrap_or(0.92);
            let rgb = DynamicImage::ImageRgba8(image).to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut output,
                (quality * 100.0).round() as u8,
            )
            .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
        }
        "image/webp" => DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut output), ImageFormat::WebP),
        _ => DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut output), ImageFormat::Png),
    };
    result.ok()?;
    (output.len() <= MAX_ENCODED_BYTES).then_some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_actual_rgba_pixels_to_supported_image_formats() {
        let pixels = [255, 0, 0, 255, 0, 0, 255, 255];
        for mime in ["image/png", "image/jpeg", "image/webp"] {
            let encoded = encode(2, 1, &pixels, mime, Some(1.0)).unwrap();
            let decoded = image::load_from_memory(&encoded).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (2, 1));
            if mime != "image/jpeg" {
                assert_eq!(decoded.to_rgba8().into_raw(), pixels);
            }
        }
    }

    #[test]
    fn rejects_zero_or_mismatched_bitmaps() {
        let args = [
            JsValue::from("canvasEncode".to_string()),
            JsValue::from(1.0),
            JsValue::from(1.0),
            JsValue::from("image/png".to_string()),
            JsValue::from(0.9),
            JsValue::Bytes(vec![1, 2, 3]),
        ];
        assert!(canvas_host_call("canvasEncode", &args).is_err());
        assert!(encode(0, 1, &[], "image/png", None).is_none());
    }

    #[test]
    fn decodes_encoded_pixels_and_rejects_invalid_sources() {
        let pixels = [9, 44, 201, 255];
        let encoded = encode(1, 1, &pixels, "image/png", None).unwrap();
        assert_eq!(decode(&encoded), Some((1, 1, pixels.to_vec())));
        assert!(decode(&[]).is_none());
        assert!(decode(b"not an image").is_none());
    }

    #[test]
    fn decodes_svg_into_straight_alpha_canvas_pixels() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><rect width="1" height="1" fill="#f00"/></svg>"##;
        let (width, height, rgba) = decode(svg).unwrap();
        assert_eq!((width, height), (2, 1));
        assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[4..], &[0, 0, 0, 0]);
    }
}
