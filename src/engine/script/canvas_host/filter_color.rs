//! CSS color functions operate on straight, gamma-encoded sRGB. Keep floating
//! intermediates across a contiguous color chain, clamp each primitive, and
//! quantize once. Matrices cannot be fused across their saturation boundaries.
//! https://drafts.csswg.org/filter-effects-1/#FilterPrimitivesOverview
use super::*;
use serde::Deserialize;
use std::borrow::Cow;

mod operation;
use operation::{Operation, Step};

const MAX_COLOR_STEPS: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    operations: Vec<Operation>,
}

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(source) = args.get(1).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    finish(args, Cow::Borrowed(source))
}

pub(super) fn paint_owned(args: &mut [JsValue]) -> JsValue {
    let Some(source) = super::owned_pixels::take(args, 1) else {
        return JsValue::Null;
    };
    finish(args, Cow::Owned(source))
}

fn finish(args: &[JsValue], source: Cow<'_, [u8]>) -> JsValue {
    let Some(JsValue::String(encoded)) = args.get(2) else {
        return JsValue::Null;
    };
    if encoded.len() > 16 * 1024 {
        return JsValue::Null;
    }
    serde_json::from_str::<Request>(encoded)
        .ok()
        .and_then(|request| render(&request, source))
        .map_or(JsValue::Null, JsValue::Bytes)
}

fn render(request: &Request, source: Cow<'_, [u8]>) -> Option<Vec<u8>> {
    let pixels = (request.width as usize).checked_mul(request.height as usize)?;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS
        || request.width > 16384
        || request.height > 16384
        || source.len() != pixels.checked_mul(4)?
        || request.operations.len() > 64
        || pixels.checked_mul(request.operations.len())? > MAX_COLOR_STEPS
    {
        return None;
    }
    // Compile and validate the whole request before touching any source byte.
    let steps: Vec<Step> = request
        .operations
        .iter()
        .map(Operation::prepare)
        .collect::<Option<_>>()?;
    let mut result = source.into_owned();
    if steps.is_empty() {
        return Some(result);
    }
    for pixel in result.chunks_exact_mut(4) {
        let mut value = [
            f64::from(pixel[0]) / 255.0,
            f64::from(pixel[1]) / 255.0,
            f64::from(pixel[2]) / 255.0,
            f64::from(pixel[3]) / 255.0,
        ];
        for step in &steps {
            value = step.apply(value);
        }
        pixel[3] = quantize(value[3]);
        for channel in 0..3 {
            pixel[channel] = if pixel[3] == 0 {
                0
            } else {
                quantize(value[channel])
            };
        }
    }
    Some(result)
}

fn quantize(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8
}

#[cfg(test)]
mod tests;
