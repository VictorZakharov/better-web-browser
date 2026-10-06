//! Owned image drawing using the existing Canvas straight-alpha sampling contract.
//! Geometry and clipping are already resolved by Canvas; no author callbacks or
//! borrowed JS storage are retained across this host operation.
use super::{JsValue, MAX_CANVAS_PIXELS, composite::Operator};
use serde::Deserialize;

mod sample;

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
    let Some(JsValue::String(encoded)) = args.get(1) else {
        return JsValue::Null;
    };
    if encoded.len() > 2048 {
        return JsValue::Null;
    }
    let Some(destination) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
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
        .and_then(|request| render(&request, destination, source, clip))
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

fn render(
    request: &Request,
    destination: &[u8],
    source: &[u8],
    clip: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let pixels = bitmap_pixels(request.width, request.height, destination)?;
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
    let mut output = destination.to_vec();
    let [a, b, c, d, e, f] = request.inverse;
    let [sx, sy, sw, sh] = request.source;
    let [dx, dy, dw, dh] = request.destination;
    for row in top..bottom {
        for column in left..right {
            let index = row as usize * request.width as usize + column as usize;
            if clip.is_some_and(|bits| bits[index / 8] & (1 << (index % 8)) == 0) {
                continue;
            }
            // Preserve the scalar evaluation order; fused stepping would alter
            // the chosen texel at fractional and transformed boundaries.
            let x = a * (f64::from(column) + 0.5) + c * (f64::from(row) + 0.5) + e;
            let y = b * (f64::from(column) + 0.5) + d * (f64::from(row) + 0.5) + f;
            if x < dx || y < dy || x >= dx + dw || y >= dy + dh {
                continue;
            }
            let sample_x = sx + (x - dx) * sw / dw - 0.5;
            let sample_y = sy + (y - dy) * sh / dh - 0.5;
            if !sample_x.is_finite()
                || !sample_y.is_finite()
                || sample_x < -0.5
                || sample_y < -0.5
                || sample_x >= f64::from(request.source_width) - 0.5
                || sample_y >= f64::from(request.source_height) - 0.5
            {
                continue;
            }
            let color = sample::pixel(
                source,
                request.source_width,
                request.source_height,
                sample_x,
                sample_y,
                request.smooth,
            );
            operator.pixel(
                &mut output[index * 4..index * 4 + 4],
                color,
                request.opacity,
            );
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests;
