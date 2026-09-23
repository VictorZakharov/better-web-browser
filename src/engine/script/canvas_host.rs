//! Bounded serialization of the Canvas-owned sRGB bitmap via the existing image codecs.

use super::binding_helpers::argument_id;
use super::*;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use std::io::Cursor;

const MAX_CANVAS_PIXELS: usize = 4 * 1024 * 1024;
const MAX_ENCODED_BYTES: usize = 24 * 1024 * 1024;

pub(super) fn canvas_host_call(operation: &str, args: &[JsValue]) -> JsResult<Option<JsValue>> {
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
}
