//! Canvas fill geometry reuses tiny-skia's compound-path winding rasterizer.
//! The input points are already in bitmap space; the painting CTM affects the
//! paint source, not the stored default path's construction-time coordinates.

use super::coverage::Region;
use super::path::{MAX_COORDINATE, MAX_POINTS, Part};
use super::*;
use resvg::tiny_skia::{FillRule, PathBuilder};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    rule: String,
    parts: Vec<Part>,
}

pub(super) fn mask(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(source)) = args.get(1) else {
        return JsValue::Null;
    };
    mask_from_source(source).map_or(JsValue::Null, JsValue::Bytes)
}

pub(super) fn mask_from_source(source: &str) -> Option<Vec<u8>> {
    if source.len() > 1024 * 1024 {
        return None;
    }
    super::mask_cache::rasterize(super::mask_cache::Kind::Fill, source, || {
        serde_json::from_str::<Request>(source)
            .ok()
            .and_then(rasterize)
    })
}

fn rasterize(request: Request) -> Option<Vec<u8>> {
    let region = Region {
        width: request.width,
        height: request.height,
        left: request.left,
        top: request.top,
        antialias: true,
    };
    region.pixels()?;
    let rule = match request.rule.as_str() {
        "nonzero" => FillRule::Winding,
        "evenodd" => FillRule::EvenOdd,
        _ => return None,
    };
    if request.parts.len() > MAX_POINTS {
        return None;
    }
    let mut builder = PathBuilder::new();
    let mut count = 0usize;
    for part in request.parts {
        count = count.checked_add(part.points.len())?;
        if count > MAX_POINTS
            || part
                .points
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
        {
            return None;
        }
        if part.points.len() < 3 {
            continue;
        }
        let first = part.points[0];
        builder.move_to(first[0], first[1]);
        for point in part.points.iter().skip(1) {
            builder.line_to(point[0], point[1]);
        }
        // HTML fills each subpath as closed, even when it remains open for a
        // later stroke. Do not mutate the author-visible path's closed state.
        builder.close();
    }
    let path = builder.finish();
    region.rasterize(path.as_ref(), rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            width: 8,
            height: 8,
            left: 0,
            top: 0,
            rule: "nonzero".into(),
            parts: vec![Part {
                points: vec![[0.5, 0.0], [2.5, 0.0], [2.5, 2.0], [0.5, 2.0]],
                closed: false,
            }],
        }
    }

    #[test]
    fn open_contour_is_closed_and_fractional_source_coverage_is_retained() {
        let mask = rasterize(request()).unwrap();
        assert!((127..=128).contains(&mask[0]));
        assert_eq!(mask[1], 255);
        assert!((127..=128).contains(&mask[2]));
        assert_eq!(mask[3], 0);
        assert_eq!(mask[16], 0);
    }

    #[test]
    fn compound_contours_are_rasterized_together_with_the_selected_rule() {
        let mut nonzero = request();
        nonzero.parts.push(Part {
            points: vec![[0.5, 0.0], [2.5, 0.0], [2.5, 2.0], [0.5, 2.0]],
            closed: true,
        });
        assert_eq!(rasterize(nonzero).unwrap()[1], 255);
        let mut evenodd = request();
        evenodd.rule = "evenodd".into();
        evenodd.parts.push(Part {
            points: vec![[0.5, 0.0], [2.5, 0.0], [2.5, 2.0], [0.5, 2.0]],
            closed: true,
        });
        assert!(rasterize(evenodd).unwrap().iter().all(|v| *v == 0));
    }

    #[test]
    fn degenerate_fill_is_supported_empty_coverage_not_fallback() {
        let mut empty = request();
        empty.parts[0].points = vec![[1.0, 1.0], [2.0, 2.0]];
        assert!(rasterize(empty).unwrap().iter().all(|v| *v == 0));
    }

    #[test]
    fn roi_origin_offsets_bitmap_geometry_without_changing_fractional_coverage() {
        let original = rasterize(request()).unwrap();
        let mut translated = request();
        translated.left = 20;
        translated.top = 30;
        for point in &mut translated.parts[0].points {
            point[0] += 20.0;
            point[1] += 30.0;
        }
        assert_eq!(rasterize(translated).unwrap(), original);
    }

    #[test]
    fn opposite_winding_cancels_including_fractional_edges() {
        let mut opposite = request();
        opposite.parts.push(Part {
            points: vec![[0.5, 0.0], [0.5, 2.0], [2.5, 2.0], [2.5, 0.0]],
            closed: false,
        });
        assert!(rasterize(opposite).unwrap().iter().all(|v| *v == 0));
    }

    #[test]
    fn invalid_native_fill_requests_fail_closed_before_allocating_masks() {
        for source in ["{}", "[]", "bad json"] {
            assert!(matches!(
                mask(&[JsValue::Null, JsValue::from(source.to_string())]),
                JsValue::Null
            ));
        }
        let mut invalid = request();
        invalid.parts[0].points[0][0] = f32::NAN;
        assert!(rasterize(invalid).is_none());
        let mut invalid = request();
        invalid.width = u32::MAX;
        assert!(rasterize(invalid).is_none());
        let mut invalid = request();
        invalid.parts[0].points = vec![[1.0, 2.0]; MAX_POINTS + 1];
        assert!(rasterize(invalid).is_none());
        let mut invalid = request();
        invalid.rule = "invalid".into();
        assert!(rasterize(invalid).is_none());
    }
}
