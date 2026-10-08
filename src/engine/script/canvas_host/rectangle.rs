//! Bounded double-coordinate solid rectangles and whole-pixel clears.
//! This ports the existing Canvas rectangle integrator, not tiny-skia's f32
//! path mask, so fractional area is not quantized to coverage bytes first.
//! https://html.spec.whatwg.org/multipage/canvas.html#drawing-rectangles-to-the-bitmap
use super::*;
use serde::Deserialize;
use std::borrow::Cow;

mod geometry;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    left: u32,
    top: u32,
    rect: [f64; 4],
    matrix: [f64; 6],
    color: Option<[f64; 4]>,
    opacity: f64,
    opaque: bool,
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
    serde_json::from_str::<Request>(encoded)
        .ok()
        .and_then(|request| {
            let clip = super::raster_clip::Clip::from_args_at_region(
                args,
                3,
                [request.width, request.height],
                [
                    i32::try_from(request.left).ok()?,
                    i32::try_from(request.top).ok()?,
                ],
            )?;
            render(&request, destination, clip.as_ref())
        })
        .map_or(JsValue::Null, JsValue::Bytes)
}

fn render(
    request: &Request,
    destination: Cow<'_, [u8]>,
    clip: Option<&super::raster_clip::Clip<'_>>,
) -> Option<Vec<u8>> {
    let pixels = (request.width as usize).checked_mul(request.height as usize)?;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS
        || request.width > 16384
        || request.height > 16384
        || request.left.checked_add(request.width)? > 16384
        || request.top.checked_add(request.height)? > 16384
        || destination.len() != pixels.checked_mul(4)?
        || !request.opacity.is_finite()
        || !(0.0..=1.0).contains(&request.opacity)
        || request
            .rect
            .iter()
            .chain(&request.matrix)
            .any(|v| !v.is_finite() || v.abs() > 16384.0)
        || request.rect[2] <= 0.0
        || request.rect[3] <= 0.0
        || request.color.is_some_and(|color| {
            color
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=255.0).contains(v))
        })
        || request.opaque && destination.chunks_exact(4).any(|pixel| pixel[3] != 255)
    {
        return None;
    }
    let [a, b, c, d, e, f] = request.matrix;
    let determinant = a * d - b * c;
    if determinant == 0.0 || !determinant.is_finite() {
        return None;
    }
    let inverse = [
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
        (c * f - d * e) / determinant,
        (b * e - a * f) / determinant,
    ];
    if inverse.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let polygon = geometry::polygon(request.rect, request.matrix);
    if polygon
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 16384.0)
    {
        return None;
    }
    let [min_x, min_y, max_x, max_y] = geometry::bounds(&polygon);
    let aligned = b == 0.0 && c == 0.0 || a == 0.0 && d == 0.0;
    // Bound convex clipping work independently of the bitmap memory limit.
    // Oversized affine paints keep the interruptible scalar fallback.
    if request.color.is_some() && !aligned && pixels > 1024 * 1024 {
        return None;
    }
    let [ia, ib, ic, id, ie, iff] = inverse;
    let [rx, ry, rw, rh] = request.rect;
    // Validate options before mutating the Rust-owned bridge allocation. If
    // bounded geometry declines later, no partial bitmap is returned to V8.
    let mut output = destination.into_owned();
    for (index, pixel) in output.chunks_exact_mut(4).enumerate() {
        if clip.is_some_and(|clip| !clip.allows(index)) {
            continue;
        }
        let x = (index % request.width as usize) as f64 + f64::from(request.left);
        let y = (index / request.width as usize) as f64 + f64::from(request.top);
        if let Some(color) = request.color {
            let coverage = if aligned {
                ((x + 1.0).min(max_x) - x.max(min_x)).max(0.0)
                    * ((y + 1.0).min(max_y) - y.max(min_y)).max(0.0)
            } else {
                geometry::area(&polygon, x, y)?
            };
            if coverage > 0.0 {
                super::solid_mask::source_over(pixel, color, request.opacity * coverage);
            }
        } else {
            let ux = ia * (x + 0.5) + ic * (y + 0.5) + ie;
            let uy = ib * (x + 0.5) + id * (y + 0.5) + iff;
            // Axis-aligned clear bounds are rounded by the caller, preserving
            // whole-pixel clearing. Other affine clears test the pixel center.
            let covered = if aligned {
                x >= (min_x + 0.5).floor()
                    && x < (max_x + 0.5).floor()
                    && y >= (min_y + 0.5).floor()
                    && y < (max_y + 0.5).floor()
            } else {
                ux >= rx && ux < rx + rw && uy >= ry && uy < ry + rh
            };
            if covered {
                pixel.fill(0);
                if request.opaque {
                    pixel[3] = 255;
                }
            }
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests;
