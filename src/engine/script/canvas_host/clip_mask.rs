//! Move the existing Canvas binary clip scanline contract off the JS pixel loop.
//! Retain f64 coordinates, center-sample winding, implicit subpath closure and
//! intersection with the previous immutable packed mask. This is not a claim
//! of antialiased clip coverage; it preserves the browser's current model.
use super::*;
use serde::Deserialize;

const MAX_WORK: u64 = 50_000_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    bounds: [u32; 4],
    rule: String,
    parts: Vec<Vec<[f64; 2]>>,
}

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(encoded)) = args.get(1) else {
        return JsValue::Null;
    };
    if encoded.len() > 1024 * 1024 {
        return JsValue::Null;
    }
    let previous = match args.get(2) {
        Some(JsValue::Null) => None,
        Some(value) => match value.as_bytes() {
            Some(bytes) => Some(bytes),
            None => return JsValue::Null,
        },
        None => return JsValue::Null,
    };
    serde_json::from_str::<Request>(encoded)
        .ok()
        .and_then(|request| rasterize(&request, previous))
        .map_or(JsValue::Null, JsValue::Bytes)
}

fn rasterize(request: &Request, previous: Option<&[u8]>) -> Option<Vec<u8>> {
    let pixels = u64::from(request.width) * u64::from(request.height);
    let [left, top, right, bottom] = request.bounds;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS as u64
        || request.width > 16384
        || request.height > 16384
        || left > right
        || top > bottom
        || right > request.width
        || bottom > request.height
        || previous.is_some_and(|bits| bits.len() != (pixels as usize).div_ceil(8))
        || request.parts.len() > super::path::MAX_POINTS
        || !matches!(request.rule.as_str(), "nonzero" | "evenodd")
    {
        return None;
    }
    let mut edges = Vec::new();
    let mut points = 0;
    for part in &request.parts {
        points += part.len();
        if points > super::path::MAX_POINTS
            || part
                .iter()
                .flatten()
                .any(|value| !value.is_finite() || value.abs() > 16384.0)
        {
            return None;
        }
        if part.len() < 2 {
            continue;
        }
        edges.extend(part.windows(2).map(|pair| (pair[0], pair[1])));
        edges.push((*part.last()?, part[0]));
    }
    if u64::from(right - left) * u64::from(bottom - top) * edges.len() as u64 > MAX_WORK {
        return None;
    }
    let mut output = vec![0; (pixels as usize).div_ceil(8)];
    let mut intersections = Vec::with_capacity(edges.len());
    for y in top..bottom {
        intersections.clear();
        let sample_y = f64::from(y) + 0.5;
        for ([x0, y0], [x1, y1]) in &edges {
            if (*y0 <= sample_y && *y1 > sample_y) || (*y1 <= sample_y && *y0 > sample_y) {
                intersections.push((
                    x0 + (sample_y - y0) * (x1 - x0) / (y1 - y0),
                    if y1 > y0 { 1_i32 } else { -1 },
                ));
            }
        }
        intersections.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut edge = 0;
        let mut winding = 0;
        for x in left..right {
            while edge < intersections.len() && intersections[edge].0 <= f64::from(x) + 0.5 {
                winding += intersections[edge].1;
                edge += 1;
            }
            let covered = if request.rule == "evenodd" {
                edge % 2 != 0
            } else {
                winding != 0
            };
            let index = y as usize * request.width as usize + x as usize;
            if covered && previous.is_none_or(|bits| bits[index / 8] & (1 << (index % 8)) != 0) {
                output[index / 8] |= 1 << (index % 8);
            }
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests;
