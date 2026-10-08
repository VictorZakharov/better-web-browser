//! Gradient source shaders reuse the locked BSD-3-Clause tiny-skia backend.
//! It interpolates straight color before premultiplication; the shared Canvas
//! compositor subsequently applies path coverage, opacity and the destination.

use super::*;
use resvg::tiny_skia::{
    Color, GradientStop, LinearGradient, Point, RadialGradient, Shader, SpreadMode, SweepGradient,
    Transform,
};
use serde::Deserialize;
use std::borrow::Cow;

const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_STOPS: usize = 256;
const MAX_SHADER_WORK: usize = 32 * 1024 * 1024;
const MAX_COORDINATE: f64 = 16384.0;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    transform: [f64; 6],
    kind: Kind,
    geometry: Vec<f64>,
    stops: Vec<Stop>,
    opacity: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Linear,
    Radial,
    Conic,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stop {
    offset: f64,
    channels: [f64; 4],
}

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(destination) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    finish(args, Cow::Borrowed(destination), false)
}

pub(super) fn paint_owned(args: &mut [JsValue]) -> JsValue {
    let Some(destination) = super::owned_pixels::take(args, 2) else {
        return JsValue::Null;
    };
    finish(args, Cow::Owned(destination), false)
}

pub(super) fn paint_path(args: &[JsValue]) -> JsValue {
    let Some(destination) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    finish(args, Cow::Borrowed(destination), true)
}

pub(super) fn paint_path_owned(args: &mut [JsValue]) -> JsValue {
    let Some(destination) = super::owned_pixels::take(args, 2) else {
        return JsValue::Null;
    };
    finish(args, Cow::Owned(destination), true)
}

fn finish(args: &[JsValue], destination: Cow<'_, [u8]>, path: bool) -> JsValue {
    let Some(JsValue::String(source)) = args.get(1) else {
        return JsValue::Null;
    };
    if source.len() > MAX_REQUEST_BYTES {
        return JsValue::Null;
    }
    serde_json::from_str::<Request>(source)
        .ok()
        .and_then(|request| {
            let clip = super::raster_clip::Clip::from_args_at_region(
                args,
                if path { 5 } else { 4 },
                [request.width, request.height],
                [request.left, request.top],
            )?;
            let mask = if path {
                super::path_shader::coverage(
                    args,
                    3,
                    [request.width, request.height],
                    [request.left, request.top],
                    destination.len(),
                )?
            } else {
                super::shader_mask::Samples::from_argument(args.get(3))?
            };
            render_clipped(&request, destination, mask, clip.as_ref())
        })
        .map_or(JsValue::Null, JsValue::Bytes)
}

#[cfg(test)]
fn render(request: &Request, destination: &[u8], mask: Option<&[u8]>) -> Option<Vec<u8>> {
    let mask = mask.map_or(
        super::shader_mask::Samples::Full,
        super::shader_mask::Samples::Bytes,
    );
    render_clipped(request, Cow::Borrowed(destination), mask, None)
}

fn render_clipped(
    request: &Request,
    destination: Cow<'_, [u8]>,
    mask: super::shader_mask::Samples<'_>,
    clip: Option<&super::raster_clip::Clip<'_>>,
) -> Option<Vec<u8>> {
    let pixels = (request.width as usize).checked_mul(request.height as usize)?;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS
        || request.width > 16384
        || request.height > 16384
        || destination.len() != pixels.checked_mul(4)?
        || pixels.checked_mul(request.stops.len().checked_add(2)?)? > MAX_SHADER_WORK
        || request.left.unsigned_abs() > 16384
        || request.top.unsigned_abs() > 16384
        || !request.opacity.is_finite()
        || !(0.0..=1.0).contains(&request.opacity)
    {
        return None;
    }
    let shader = shader(request)?;
    if request.stops.len() > 1 && matches!(shader, Shader::SolidColor(_)) {
        // Provider near-degenerate shortcuts need not match double geometry.
        // Preserve the scalar implementation rather than silently flatten it.
        return None;
    }
    super::shader_mask::render(
        request.width,
        request.height,
        shader,
        destination,
        mask,
        request.opacity,
        clip,
    )
}

fn shader(request: &Request) -> Option<Shader<'static>> {
    let expected = match request.kind {
        Kind::Linear => 4,
        Kind::Radial => 6,
        Kind::Conic => 3,
    };
    if request.geometry.len() != expected
        || request.geometry.iter().any(|v| !v.is_finite())
        || request
            .transform
            .iter()
            .any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
        || request.stops.is_empty()
        || request.stops.len() > MAX_STOPS
        || request
            .stops
            .windows(2)
            .any(|pair| pair[0].offset > pair[1].offset)
    {
        return None;
    }
    for (index, value) in request.geometry.iter().enumerate() {
        if !(matches!(request.kind, Kind::Conic) && index == 0) && value.abs() > MAX_COORDINATE {
            return None;
        }
    }
    let mut stops = request
        .stops
        .iter()
        .map(|stop| {
            if !stop.offset.is_finite()
                || !(0.0..=1.0).contains(&stop.offset)
                || stop
                    .channels
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=255.0).contains(v))
            {
                return None;
            }
            let [r, g, b, a] = stop.channels.map(|v| (v / 255.0) as f32);
            Some(GradientStop::new(
                stop.offset as f32,
                Color::from_rgba(r, g, b, a)?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    let [a, b, c, d, e, f] = request.transform.map(|v| v as f32);
    if f64::from(a) * f64::from(d) - f64::from(b) * f64::from(c) == 0.0 {
        return None;
    }
    let transform =
        Transform::from_row(a, b, c, d, e - request.left as f32, f - request.top as f32);
    if !transform.invert()?.is_finite() {
        return None;
    }
    let g = &request.geometry;
    match request.kind {
        Kind::Linear => {
            if g[0] == g[2] && g[1] == g[3] {
                return None;
            }
            LinearGradient::new(
                Point::from_xy(g[0] as f32, g[1] as f32),
                Point::from_xy(g[2] as f32, g[3] as f32),
                stops,
                SpreadMode::Pad,
                transform,
            )
        }
        Kind::Radial => {
            if g[2] < 0.0 || g[5] < 0.0 || (g[0] == g[3] && g[1] == g[4] && g[2] == g[5]) {
                return None;
            }
            // tiny-skia optimizes a single stop to SolidColor before testing
            // the cone. Duplicate it to retain radial geometry outside the cone.
            if stops.len() == 1 {
                stops.push(stops[0]);
            }
            RadialGradient::new(
                Point::from_xy(g[0] as f32, g[1] as f32),
                g[2] as f32,
                Point::from_xy(g[3] as f32, g[4] as f32),
                g[5] as f32,
                stops,
                SpreadMode::Pad,
                transform,
            )
        }
        Kind::Conic => {
            let x = g[1] as f32;
            let y = g[2] as f32;
            let angle = (g[0] % std::f64::consts::TAU).to_degrees() as f32;
            SweepGradient::new(
                Point::from_xy(x, y),
                0.0,
                360.0,
                stops,
                SpreadMode::Pad,
                transform.pre_rotate_at(angle, x, y),
            )
        }
    }
}

#[cfg(test)]
#[path = "gradient_mask/tests.rs"]
mod tests;
