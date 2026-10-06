//! Stroke geometry uses the existing BSD-3-Clause tiny-skia dependency (via resvg).
//! Paint, clipping and compositing remain owned by the Canvas context. Unsupported
//! numeric ranges return null, selecting the bounded software fallback, not blank pixels.

use super::*;
use resvg::tiny_skia::FillRule;
use serde::Deserialize;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
pub(super) const MAX_POINTS: usize = 8192;
pub(super) const MAX_COORDINATE: f32 = 16384.0;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    pub line_width: f32,
    pub miter_limit: f32,
    pub cap: String,
    pub join: String,
    #[serde(default = "identity")]
    pub transform: [f32; 6],
    pub parts: Vec<Part>,
    #[serde(default)]
    pub dash: Vec<f64>,
    #[serde(default)]
    pub dash_offset: f64,
    #[serde(default)]
    antialias: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Part {
    pub points: Vec<[f32; 2]>,
    pub closed: bool,
}

pub(super) fn stroke_contains(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(source)) = args.get(1) else {
        return JsValue::Null;
    };
    let Some(point) = args
        .get(2)
        .and_then(JsValue::as_number)
        .zip(args.get(3).and_then(JsValue::as_number))
    else {
        return JsValue::Null;
    };
    if source.len() > MAX_REQUEST_BYTES || !point.0.is_finite() || !point.1.is_finite() {
        return JsValue::Null;
    }
    let Some(path) = serde_json::from_str::<Request>(source)
        .ok()
        .and_then(|request| super::stroke_outline::build(&request))
    else {
        return JsValue::Null;
    };
    JsValue::Boolean(super::stroke_outline::contains(&path, point))
}

pub(super) fn stroke_mask(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(source)) = args.get(1) else {
        return JsValue::Null;
    };
    mask_from_source(source).map_or(JsValue::Null, JsValue::Bytes)
}

pub(super) fn mask_from_source(source: &str) -> Option<Vec<u8>> {
    if source.len() > MAX_REQUEST_BYTES {
        return None;
    }
    super::mask_cache::rasterize(super::mask_cache::Kind::Stroke, source, || {
        serde_json::from_str::<Request>(source)
            .ok()
            .and_then(rasterize)
    })
}

fn rasterize(request: Request) -> Option<Vec<u8>> {
    let region = super::coverage::Region {
        width: request.width,
        height: request.height,
        left: request.left,
        top: request.top,
        antialias: request.antialias,
    };
    region.pixels()?;
    let path = super::stroke_outline::build(&request)?;
    match &path {
        super::stroke_outline::Outline::Path(path) => {
            region.rasterize(Some(path), FillRule::Winding)
        }
        super::stroke_outline::Outline::Empty => region.rasterize(None, FillRule::Winding),
    }
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
            dash: vec![],
            dash_offset: 0.0,
            antialias: false,
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

    #[test]
    fn antialiased_mask_preserves_fractional_source_coverage_and_roi_origin() {
        let mut req = request();
        req.antialias = true;
        req.line_width = 1.0;
        let mask = rasterize(req).unwrap();
        assert!((127..=128).contains(&mask[7 * 16 + 8]));
        assert!((127..=128).contains(&mask[8 * 16 + 8]));
        assert_eq!(mask[6 * 16 + 8], 0);
        assert_eq!(mask[9 * 16 + 8], 0);
        let mut shifted = request();
        shifted.antialias = true;
        shifted.line_width = 1.0;
        shifted.left = 4;
        shifted.top = 4;
        let shifted = rasterize(shifted).unwrap();
        assert_eq!(shifted[3 * 16 + 4], mask[7 * 16 + 8]);
        assert_eq!(shifted[4 * 16 + 4], mask[8 * 16 + 8]);
    }

    #[test]
    fn edge_coverage_is_unioned_before_source_opacity_is_applied() {
        let mut single = request();
        single.antialias = true;
        single.line_width = 1.0;
        let expected = rasterize(single).unwrap();
        let mut repeated = request();
        repeated.antialias = true;
        repeated.line_width = 1.0;
        repeated.parts.push(Part {
            points: vec![[4.0, 8.0], [12.0, 8.0]],
            closed: false,
        });
        assert_eq!(rasterize(repeated).unwrap(), expected);
    }
}
