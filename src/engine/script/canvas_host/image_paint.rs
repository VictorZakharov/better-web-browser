//! Owned image drawing using the existing Canvas straight-alpha sampling contract.
//! Geometry and clipping are already resolved by Canvas; no author callbacks or
//! borrowed JS storage are retained across this host operation.
use super::{JsValue, MAX_CANVAS_PIXELS, composite::Operator};
use serde::Deserialize;
use std::borrow::Cow;

mod blit;
pub(super) mod sample;
mod scalar;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
    bounds: [u32; 4],
    inverse: [f64; 6],
    source: [f64; 4],
    destination: [f64; 4],
    opacity: f64,
    smooth: bool,
    operator: String,
}

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(destination) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    paint_destination(args, Cow::Borrowed(destination))
}

pub(super) fn paint_owned(args: &mut [JsValue]) -> JsValue {
    let Some(destination) = super::owned_pixels::take(args, 2) else {
        return JsValue::Null;
    };
    paint_destination(args, Cow::Owned(destination))
}

fn paint_destination(args: &[JsValue], destination: Cow<'_, [u8]>) -> JsValue {
    let Some(JsValue::String(encoded)) = args.get(1) else {
        return JsValue::Null;
    };
    if encoded.len() > 2048 {
        return JsValue::Null;
    }
    let Some(source) = args.get(3).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    let clip = match args.get(4) {
        Some(JsValue::Null) => None,
        Some(value) => match value.as_bytes() {
            Some(bytes) => Some(bytes),
            None => return JsValue::Null,
        },
        None => return JsValue::Null,
    };
    serde_json::from_str::<Request>(encoded)
        .ok()
        .and_then(|request| render_destination(&request, destination, source, clip))
        .map_or(JsValue::Null, JsValue::Bytes)
}

fn bitmap_pixels(width: u32, height: u32, bytes: &[u8]) -> Option<usize> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    (width > 0
        && height > 0
        && pixels <= MAX_CANVAS_PIXELS
        && bytes.len() == pixels.checked_mul(4)?)
    .then_some(pixels)
}

#[cfg(test)]
fn render(
    request: &Request,
    destination: &[u8],
    source: &[u8],
    clip: Option<&[u8]>,
) -> Option<Vec<u8>> {
    render_destination(request, Cow::Borrowed(destination), source, clip)
}

fn render_destination(
    request: &Request,
    destination: Cow<'_, [u8]>,
    source: &[u8],
    clip: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let pixels = bitmap_pixels(request.width, request.height, &destination)?;
    bitmap_pixels(request.source_width, request.source_height, source)?;
    let operator = Operator::parse(&request.operator)?;
    let [left, top, right, bottom] = request.bounds;
    if left > right
        || top > bottom
        || right > request.width
        || bottom > request.height
        || clip.is_some_and(|bits| bits.len() != pixels.div_ceil(8))
        || !request.opacity.is_finite()
        || !(0.0..=1.0).contains(&request.opacity)
        || request
            .inverse
            .iter()
            .chain(&request.source)
            .chain(&request.destination)
            .any(|value| !value.is_finite())
        || request.source[2] <= 0.0
        || request.source[3] <= 0.0
        || request.destination[2] <= 0.0
        || request.destination[3] <= 0.0
    {
        return None;
    }
    let mut output = destination.into_owned();
    if !blit::paint(request, &mut output, source, clip, operator) {
        scalar::paint(request, &mut output, source, clip, operator);
    }
    Some(output)
}

#[cfg(test)]
mod tests;
