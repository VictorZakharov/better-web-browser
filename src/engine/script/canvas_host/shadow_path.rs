//! Rasterize a solid path and its shadow in one owned transaction. Geometry
//! remains tiny-skia's existing validated coverage; no full source bitmap
//! crosses the JS/native bridge merely to paint a small shadowed stroke.

use super::*;
use serde::Deserialize;
use std::borrow::Cow;

#[derive(Deserialize)]
struct Region {
    width: u32,
    height: u32,
    left: u32,
    top: u32,
}

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    render(args).map_or(JsValue::Null, JsValue::Bytes)
}

fn render(args: &[JsValue]) -> Option<Vec<u8>> {
    let destination = args.get(1)?.as_bytes()?;
    render_destination(args, Cow::Borrowed(destination))
}

pub(super) fn paint_owned(args: &mut [JsValue]) -> JsValue {
    let Some(destination) = super::owned_pixels::take(args, 1) else {
        return JsValue::Null;
    };
    // This allocation was copied from V8 by the bridge. It is not an author's
    // ArrayBuffer, even if the author passed the same view for destination/clip.
    render_destination(args, Cow::Owned(destination)).map_or(JsValue::Null, JsValue::Bytes)
}

fn render_destination(args: &[JsValue], destination: Cow<'_, [u8]>) -> Option<Vec<u8>> {
    let JsValue::String(request) = args.get(2)? else {
        return None;
    };
    if request.len() > 1024 * 1024 {
        return None;
    }
    let region: Region = serde_json::from_str(request).ok()?;
    let width = super::source_layer::dimension(args.get(5)?)?;
    let height = super::source_layer::dimension(args.get(6)?)?;
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels > MAX_CANVAS_PIXELS
        || destination.len() != pixels.checked_mul(4)?
        || region.width == 0
        || region.height == 0
        || region.left.checked_add(region.width)? > width
        || region.top.checked_add(region.height)? > height
    {
        return None;
    }
    let JsValue::String(kind) = args.get(12)? else {
        return None;
    };
    let JsValue::Array(channels) = args.get(13)? else {
        return None;
    };
    if channels.len() != 4 {
        return None;
    }
    let mut color = [0.0; 4];
    for (output, input) in color.iter_mut().zip(channels) {
        *output = input.as_number()?;
        if !output.is_finite() || !(0.0..=255.0).contains(output) {
            return None;
        }
    }
    let opacity = args.get(14)?.as_number()?;
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return None;
    }
    let mask = match kind.as_str() {
        "fill" => super::fill::coverage_from_source(request)?,
        "stroke" => super::path::coverage_from_source(request)?,
        _ => return None,
    };
    if mask.len() != region.width as usize * region.height as usize {
        return None;
    }
    let mut source = vec![0; region.width as usize * region.height as usize * 4];
    mask.runs(|start, coverage| {
        for (pixel, coverage) in source[start * 4..(start + coverage.len()) * 4]
            .chunks_exact_mut(4)
            .zip(coverage)
        {
            if *coverage != 0 {
                super::solid_mask::source_over(
                    pixel,
                    color,
                    opacity * (f64::from(*coverage) / 255.0),
                );
            }
        }
    });
    // The final drawing clip is intentionally applied only after generating
    // coverage and shadow, exactly as the ordinary source-layer path does.
    super::source_layer::render_region_destination(
        args,
        &super::shadow::Layer {
            bounds: [
                region.left,
                region.top,
                region.left + region.width,
                region.top + region.height,
            ],
            pixels: source,
        },
        destination,
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod owned_tests;
