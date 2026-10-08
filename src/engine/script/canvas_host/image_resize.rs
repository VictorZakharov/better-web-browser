//! ImageBitmap RGBA8 resize reuses the admitted drawImage sampler rather than
//! building per-pixel JavaScript arrays. Bitmap ownership/taint stays in JS.
use super::{JsValue, MAX_CANVAS_PIXELS, image_paint::sample};
use serde::Deserialize;
use std::borrow::Cow;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
    smooth: bool,
}

pub(super) fn resize(args: &[JsValue]) -> JsValue {
    let Some(source) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    finish(args, Cow::Borrowed(source))
}

pub(super) fn resize_owned(args: &mut [JsValue]) -> JsValue {
    let Some(source) = super::owned_pixels::take(args, 2) else {
        return JsValue::Null;
    };
    finish(args, Cow::Owned(source))
}

fn finish(args: &[JsValue], source: Cow<'_, [u8]>) -> JsValue {
    let Some(JsValue::String(encoded)) = args.get(1) else {
        return JsValue::Null;
    };
    if encoded.len() > 1024 {
        return JsValue::Null;
    }
    serde_json::from_str::<Request>(encoded)
        .ok()
        .and_then(|request| render(&request, &source))
        .map_or(JsValue::Null, JsValue::Bytes)
}

fn bytes(width: u32, height: u32) -> Option<usize> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    (width > 0 && height > 0 && pixels <= MAX_CANVAS_PIXELS).then_some(pixels.checked_mul(4)?)
}

fn render(request: &Request, source: &[u8]) -> Option<Vec<u8>> {
    let source_bytes = bytes(request.source_width, request.source_height)?;
    let output_bytes = bytes(request.width, request.height)?;
    if source.len() != source_bytes {
        return None;
    }
    // Both images are at most four Mi pixels, so each invocation evaluates
    // at most sixteen Mi bilinear taps and owns at most 32 MiB of byte storage.
    let mut output = vec![0; output_bytes];
    for row in 0..request.height {
        let y = (f64::from(row) + 0.5) * f64::from(request.source_height)
            / f64::from(request.height)
            - 0.5;
        for column in 0..request.width {
            let x = (f64::from(column) + 0.5) * f64::from(request.source_width)
                / f64::from(request.width)
                - 0.5;
            let value = sample::pixel(
                source,
                request.source_width,
                request.source_height,
                x,
                y,
                request.smooth,
            );
            let offset = (row as usize * request.width as usize + column as usize) * 4;
            for channel in 0..4 {
                // Uint8ClampedArray uses round-to-even at half-way values.
                output[offset + channel] = value[channel].clamp(0.0, 255.0).round_ties_even() as u8;
            }
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests;
