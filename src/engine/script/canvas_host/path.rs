//! Stroke geometry uses the existing BSD-3-Clause tiny-skia dependency (via resvg).
//! Paint, clipping and compositing remain owned by the Canvas context. Unsupported
//! numeric ranges return null, selecting the bounded software fallback, not blank pixels.

use super::*;
use resvg::tiny_skia::{FillRule, LineCap, LineJoin, Mask, PathBuilder, Stroke, Transform};
use serde::Deserialize;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_POINTS: usize = 8192;
const MAX_COORDINATE: f32 = 16384.0;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    line_width: f32,
    miter_limit: f32,
    cap: String,
    join: String,
    #[serde(default = "identity")]
    transform: [f32; 6],
    parts: Vec<Part>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    points: Vec<[f32; 2]>,
    closed: bool,
}

pub(super) fn stroke_mask(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(source)) = args.get(1) else {
        return JsValue::Null;
    };
    if source.len() > MAX_REQUEST_BYTES {
        return JsValue::Null;
    }
    let Some(mask) = serde_json::from_str::<Request>(source)
        .ok()
        .and_then(rasterize)
    else {
        return JsValue::Null;
    };
    JsValue::Bytes(mask)
}

fn rasterize(request: Request) -> Option<Vec<u8>> {
    let pixels = (request.width as usize).checked_mul(request.height as usize)?;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS
        || request.width > 16384
        || request.height > 16384
        || request.left.unsigned_abs() > 16384
        || request.top.unsigned_abs() > 16384
        || !request.line_width.is_finite()
        || !(0.0..=4096.0).contains(&request.line_width)
        || request.line_width == 0.0
        || !request.miter_limit.is_finite()
        || !(1.0..=64.0).contains(&request.miter_limit)
        || request.parts.len() > MAX_POINTS
        || request
            .transform
            .iter()
            .any(|value| !value.is_finite() || value.abs() > MAX_COORDINATE)
    {
        return None;
    }
    let mut point_count = 0usize;
    let mut builder = PathBuilder::new();
    for part in request.parts {
        point_count = point_count.checked_add(part.points.len())?;
        if point_count > MAX_POINTS
            || part
                .points
                .iter()
                .flatten()
                .any(|coordinate| !coordinate.is_finite() || coordinate.abs() > MAX_COORDINATE)
        {
            return None;
        }
        let Some(first) = part.points.first() else {
            continue;
        };
        builder.move_to(first[0], first[1]);
        for point in part.points.iter().skip(1) {
            builder.line_to(point[0], point[1]);
        }
        if part.closed {
            builder.close();
        }
    }
    let stroke = Stroke {
        width: request.line_width,
        miter_limit: request.miter_limit,
        line_cap: match request.cap.as_str() {
            "butt" => LineCap::Butt,
            "round" => LineCap::Round,
            "square" => LineCap::Square,
            _ => return None,
        },
        line_join: match request.join.as_str() {
            "miter" => LineJoin::Miter,
            "round" => LineJoin::Round,
            "bevel" => LineJoin::Bevel,
            _ => return None,
        },
        dash: None,
    };
    let [a, b, c, d, e, f] = request.transform;
    let determinant = a * d - b * c;
    if !determinant.is_finite() || determinant == 0.0 {
        return None;
    }
    let transform = Transform::from_row(a, b, c, d, e, f);
    if !transform.invert()?.is_finite() {
        return None;
    }
    // Stroke in the painting coordinate system, then transform the outline.
    // Applying the matrix to just the centerline loses non-uniform pen geometry.
    let resolution = a.hypot(b).max(c.hypot(d)).clamp(1.0, 256.0);
    let path = builder
        .finish()?
        .stroke(&stroke, resolution)?
        .transform(transform)?;
    let mut mask = Mask::new(request.width, request.height)?;
    // Preserve the existing center-sample coverage contract for now. Native
    // antialiasing requires coordinating coverage with clips and layer effects.
    mask.fill_path(
        &path,
        FillRule::Winding,
        false,
        Transform::from_translate(-(request.left as f32), -(request.top as f32)),
    );
    Some(mask.take())
}

fn identity() -> [f32; 6] {
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            width: 16,
            height: 16,
            left: 0,
            top: 0,
            line_width: 4.0,
            miter_limit: 10.0,
            cap: "butt".into(),
            join: "miter".into(),
            transform: identity(),
            parts: vec![Part {
                points: vec![[4.0, 8.0], [12.0, 8.0]],
                closed: false,
            }],
        }
    }

    #[test]
    fn stroke_mask_has_actual_coverage_and_cap_geometry() {
        let mask = rasterize(request()).unwrap();
        assert_eq!(mask.len(), 256);
        assert_eq!(mask[8 * 16 + 8], 255);
        assert_eq!(mask[8 * 16 + 3], 0);
        let mut round = request();
        round.cap = "round".into();
        assert_eq!(rasterize(round).unwrap()[8 * 16 + 3], 255);
    }

    #[test]
    fn malformed_or_unbounded_native_requests_select_fallback() {
        for source in ["{}", "[]", "not json"] {
            assert!(matches!(
                stroke_mask(&[JsValue::Null, JsValue::from(source.to_string())]),
                JsValue::Null
            ));
        }
        let mut invalid = request();
        invalid.width = u32::MAX;
        assert!(rasterize(invalid).is_none());
        let mut invalid = request();
        invalid.parts[0].points[0][0] = f32::NAN;
        assert!(rasterize(invalid).is_none());
        let mut invalid = request();
        invalid.parts[0].points = vec![[1.0, 1.0]; MAX_POINTS + 1];
        assert!(rasterize(invalid).is_none());
    }

    #[test]
    fn transformed_native_pen_is_rasterized_after_outline_construction() {
        let mut scaled = request();
        scaled.width = 32;
        scaled.height = 48;
        scaled.transform = [2.0, 0.0, 0.0, 3.0, 0.0, 0.0];
        let mask = rasterize(scaled).unwrap();
        assert_eq!(mask[19 * 32 + 16], 255);
        assert_eq!(mask[17 * 32 + 16], 0);
        assert_eq!(mask[24 * 32 + 25], 0);
    }

    #[test]
    fn invalid_or_singular_native_transform_selects_bounded_fallback() {
        for matrix in [
            [1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            [f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 1.0, f32::INFINITY, 0.0],
            [MAX_COORDINATE + 1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ] {
            let mut invalid = request();
            invalid.transform = matrix;
            assert!(rasterize(invalid).is_none(), "{matrix:?}");
        }
    }
}
