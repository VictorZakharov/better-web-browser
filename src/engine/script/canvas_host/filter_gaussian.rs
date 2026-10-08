//! Filter Effects blur() uses its length as sigma, unlike Canvas shadowBlur/2.
//! Reuse image-rs's pinned MIT/Apache-2.0 Gaussian implementation on premultiplied
//! sRGB channels. Transparent padding supplies the infinite input's zero border.
//! https://drafts.csswg.org/filter-effects-1/#funcdef-filter-blur
use super::*;
use serde::Deserialize;
use std::borrow::Cow;

const MAX_WORKING_PIXELS: usize = 4 * 1024 * 1024;
const MAX_SAMPLE_WORK: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    sigma: f64,
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
    if encoded.len() > 1024 {
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
        || source.len() != pixels.checked_mul(4)?
        || request.width > 16384
        || request.height > 16384
        || !request.sigma.is_finite()
        || request.sigma < 0.0
    {
        return None;
    }
    // Avoid image-rs's sigma=0 default and subnormal parameter assertion. At
    // this deviation neighboring samples underflow even in f64; this is identity.
    if request.sigma < 0.01 {
        return Some(source.into_owned());
    }
    // Do not silently clamp a CSS filter's sigma to Canvas shadow's permitted
    // implementation cap. Unsupported working sets decline before allocation.
    if request.sigma > 64.0 {
        return None;
    }
    let padding = (request.sigma * 4.0).ceil() as u32 + 2;
    let width = request.width.checked_add(padding.checked_mul(2)?)?;
    let height = request.height.checked_add(padding.checked_mul(2)?)?;
    let working_pixels = (width as usize).checked_mul(height as usize)?;
    let taps = (request.sigma * 8.0).ceil() as u64 + 5;
    if working_pixels > MAX_WORKING_PIXELS
        || (working_pixels as u64).checked_mul(taps)?.checked_mul(8)? > MAX_SAMPLE_WORK
    {
        return None;
    }
    let mut input = ImageBuffer::<Rgba<f32>, Vec<f32>>::new(width, height);
    for (index, pixel) in source.chunks_exact(4).enumerate() {
        let alpha = f32::from(pixel[3]) / 255.0;
        input.put_pixel(
            index as u32 % request.width + padding,
            index as u32 / request.width + padding,
            Rgba([
                f32::from(pixel[0]) / 255.0 * alpha,
                f32::from(pixel[1]) / 255.0 * alpha,
                f32::from(pixel[2]) / 255.0 * alpha,
                alpha,
            ]),
        );
    }
    // Filter functions explicitly operate in gamma-encoded sRGB, not the SVG
    // filter primitives' default linearRGB. The library receives that space.
    let filtered = image::imageops::blur(&input, request.sigma as f32);
    let mut result = source.into_owned();
    for (index, output) in result.chunks_exact_mut(4).enumerate() {
        let value = filtered
            .get_pixel(
                index as u32 % request.width + padding,
                index as u32 / request.width + padding,
            )
            .0;
        output[3] = (f64::from(value[3]) * 255.0)
            .round_ties_even()
            .clamp(0.0, 255.0) as u8;
        for channel in 0..3 {
            output[channel] = if output[3] == 0 {
                0
            } else {
                (f64::from(value[channel]) / f64::from(value[3]) * 255.0)
                    .round_ties_even()
                    .clamp(0.0, 255.0) as u8
            };
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests;
