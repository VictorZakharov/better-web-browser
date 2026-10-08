//! Repeating bitmap shaders reuse the existing BSD-3-Clause tiny-skia backend.
use super::*;
use resvg::tiny_skia::{FilterQuality, Pattern, Pixmap, SpreadMode, Transform};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    source_width: u32,
    source_height: u32,
    transform: [f64; 6],
    opacity: f64,
}

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(encoded)) = args.get(1) else {
        return JsValue::Null;
    };
    if encoded.len() > 2048 {
        return JsValue::Null;
    }
    let Some(destination) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    let mask = match args.get(3) {
        Some(JsValue::Null) => None,
        Some(JsValue::Bytes(v)) => Some(v.as_slice()),
        _ => return JsValue::Null,
    };
    let Some(source) = args.get(4).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    serde_json::from_str::<Request>(encoded)
        .ok()
        .and_then(|request| {
            let clip = super::raster_clip::Clip::from_args_at_region(
                args,
                5,
                [request.width, request.height],
                [request.left, request.top],
            )?;
            render_clipped(&request, destination, mask, source, clip.as_ref())
        })
        .map_or(JsValue::Null, JsValue::Bytes)
}

#[cfg(test)]
fn render(
    request: &Request,
    destination: &[u8],
    mask: Option<&[u8]>,
    source: &[u8],
) -> Option<Vec<u8>> {
    render_clipped(request, destination, mask, source, None)
}

fn render_clipped(
    request: &Request,
    destination: &[u8],
    mask: Option<&[u8]>,
    source: &[u8],
    clip: Option<&super::raster_clip::Clip<'_>>,
) -> Option<Vec<u8>> {
    let source_pixels =
        (request.source_width as usize).checked_mul(request.source_height as usize)?;
    if source_pixels == 0
        || source_pixels > 1024 * 1024
        || request.source_width > 16384
        || request.source_height > 16384
        || source.len() != source_pixels.checked_mul(4)?
        || request.left.unsigned_abs() > 16384
        || request.top.unsigned_abs() > 16384
        || request
            .transform
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 16384.0)
    {
        return None;
    }
    let [a, b, c, d, e, f] = request.transform.map(|v| v as f32);
    if f64::from(a) * f64::from(d) - f64::from(b) * f64::from(c) == 0.0 {
        return None;
    }
    let transform =
        Transform::from_row(a, b, c, d, e - request.left as f32, f - request.top as f32);
    if !transform.invert()?.is_finite() {
        return None;
    }
    let mut image = Pixmap::new(request.source_width, request.source_height)?;
    for (input, pixel) in source
        .chunks_exact(4)
        .zip(image.data_mut().chunks_exact_mut(4))
    {
        let alpha = u16::from(input[3]);
        for channel in 0..3 {
            pixel[channel] = ((u16::from(input[channel]) * alpha + 127) / 255) as u8;
        }
        pixel[3] = input[3];
    }
    let shader = Pattern::new(
        image.as_ref(),
        SpreadMode::Repeat,
        FilterQuality::Nearest,
        1.0,
        transform,
    );
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

#[cfg(test)]
mod tests;
