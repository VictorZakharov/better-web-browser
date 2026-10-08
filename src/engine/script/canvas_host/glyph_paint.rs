//! Paint an owned shaped-glyph packet without crossing V8 once per destination
//! pixel. Shaping and glyph placement are already resolved; this operation has
//! no author callbacks and commits no partial result on malformed input.

use super::{JsValue, MAX_CANVAS_PIXELS, composite::Operator};
use serde::Deserialize;
use std::borrow::Cow;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    region: [u32; 4],
    inverse: [f64; 6],
    scale: f64,
    opacity: f64,
    operator: String,
    paint: [f64; 4],
    opaque: bool,
    stroke: bool,
}

struct Glyph<'a> {
    x: f64,
    y: f64,
    width: u32,
    height: u32,
    bounds: [u32; 4],
    color: bool,
    data: &'a [u8],
}

fn integer(value: &JsValue) -> Option<u32> {
    let number = value.as_number()?;
    (number.is_finite() && number >= 0.0 && number <= f64::from(u32::MAX) && number.fract() == 0.0)
        .then_some(number as u32)
}

fn glyph(value: &JsValue) -> Option<Glyph<'_>> {
    let JsValue::Array(parts) = value else {
        return None;
    };
    if parts.len() != 7 {
        return None;
    }
    let x = parts[0].as_number()?;
    let y = parts[1].as_number()?;
    let width = integer(&parts[2])?;
    let height = integer(&parts[3])?;
    let JsValue::Array(bounds) = &parts[4] else {
        return None;
    };
    if bounds.len() != 4 {
        return None;
    }
    let bounds = [
        integer(&bounds[0])?,
        integer(&bounds[1])?,
        integer(&bounds[2])?,
        integer(&bounds[3])?,
    ];
    let color = parts[5].as_boolean()?;
    let data = parts[6].as_bytes()?;
    let pixels = u64::from(width) * u64::from(height);
    if !x.is_finite()
        || !y.is_finite()
        || width == 0
        || height == 0
        || width > crate::limits::MAX_GLYPH_RASTER_DIMENSION
        || height > crate::limits::MAX_GLYPH_RASTER_DIMENSION
        || pixels > crate::limits::MAX_GLYPH_RASTER_PIXELS
        || data.len() as u64 != pixels * if color { 4 } else { 1 }
    {
        return None;
    }
    Some(Glyph {
        x,
        y,
        width,
        height,
        bounds,
        color,
        data,
    })
}

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(destination) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    finish(args, Cow::Borrowed(destination))
}

pub(super) fn paint_owned(args: &mut [JsValue]) -> JsValue {
    let Some(destination) = super::owned_pixels::take(args, 2) else {
        return JsValue::Null;
    };
    finish(args, Cow::Owned(destination))
}

fn finish(args: &[JsValue], destination: Cow<'_, [u8]>) -> JsValue {
    let Some(JsValue::String(encoded)) = args.get(1) else {
        return JsValue::Null;
    };
    if encoded.len() > 2048 {
        return JsValue::Null;
    }
    let Some(JsValue::Array(glyphs)) = args.get(3) else {
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
        .and_then(|request| render_destination(&request, destination, glyphs, clip))
        .map_or(JsValue::Null, JsValue::Bytes)
}

#[cfg(test)]
fn render(
    request: &Request,
    destination: &[u8],
    glyphs: &[JsValue],
    clip: Option<&[u8]>,
) -> Option<Vec<u8>> {
    render_destination(request, Cow::Borrowed(destination), glyphs, clip)
}

fn render_destination(
    request: &Request,
    destination: Cow<'_, [u8]>,
    glyphs: &[JsValue],
    clip: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let pixels = (request.width as usize).checked_mul(request.height as usize)?;
    let [region_left, region_top, region_right, region_bottom] = request.region;
    if region_left >= region_right
        || region_top >= region_bottom
        || region_right > request.width
        || region_bottom > request.height
    {
        return None;
    }
    let region_width = (region_right - region_left) as usize;
    let region_pixels = region_width.checked_mul((region_bottom - region_top) as usize)?;
    let operator = Operator::parse(&request.operator)?;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS
        || destination.len() != region_pixels.checked_mul(4)?
        || glyphs.len() > 4096
        || clip.is_some_and(|clip| clip.len() != pixels.div_ceil(8))
        || !request.scale.is_finite()
        || request.scale <= 0.0
        || !request.opacity.is_finite()
        || !(0.0..=1.0).contains(&request.opacity)
        || request.inverse.iter().any(|value| !value.is_finite())
        || request
            .paint
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=255.0).contains(value))
        || request.opaque && destination.chunks_exact(4).any(|pixel| pixel[3] != 255)
    {
        return None;
    }
    // Validate the entire packet before allocating or painting any destination.
    let glyphs = glyphs.iter().map(glyph).collect::<Option<Vec<_>>>()?;
    let mut byte_count = 0usize;
    let mut work = 0u64;
    for glyph in &glyphs {
        let [left, top, right, bottom] = glyph.bounds;
        if left > right || top > bottom || right > request.width || bottom > request.height {
            return None;
        }
        byte_count = byte_count.checked_add(glyph.data.len())?;
        work = work.checked_add(u64::from(right - left) * u64::from(bottom - top))?;
        if byte_count > 16 * 1024 * 1024 || work > 64 * 1024 * 1024 {
            return None;
        }
    }
    let mut output = destination.into_owned();
    let [a, b, c, d, e, f] = request.inverse;
    for glyph in glyphs {
        let [left, top, right, bottom] = glyph.bounds;
        for py in top.max(region_top)..bottom.min(region_bottom) {
            for px in left.max(region_left)..right.min(region_right) {
                let index = py as usize * request.width as usize + px as usize;
                if clip.is_some_and(|bits| bits[index / 8] & (1 << (index % 8)) == 0) {
                    continue;
                }
                // Keep the scalar evaluation order at transformed texel boundaries.
                let ux = a * (f64::from(px) + 0.5) + c * (f64::from(py) + 0.5) + e;
                let uy = b * (f64::from(px) + 0.5) + d * (f64::from(py) + 0.5) + f;
                let sx = ((ux - glyph.x) / request.scale).floor();
                let sy = (uy - glyph.y).floor();
                if sx < 0.0
                    || sy < 0.0
                    || sx >= f64::from(glyph.width)
                    || sy >= f64::from(glyph.height)
                    || !sx.is_finite()
                    || !sy.is_finite()
                {
                    continue;
                }
                let source = sy as usize * glyph.width as usize + sx as usize;
                let coverage = if glyph.color {
                    glyph.data[source * 4 + 3]
                } else {
                    glyph.data[source]
                };
                if coverage == 0 {
                    continue;
                }
                let mut color = request.paint;
                if glyph.color && !request.stroke {
                    color =
                        std::array::from_fn(|channel| f64::from(glyph.data[source * 4 + channel]));
                } else if !glyph.color {
                    color[3] *= f64::from(coverage) / 255.0;
                }
                let region_index =
                    (py - region_top) as usize * region_width + (px - region_left) as usize;
                operator.pixel_with_alpha(
                    &mut output[region_index * 4..region_index * 4 + 4],
                    color,
                    request.opacity,
                    request.opaque,
                );
            }
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests;
