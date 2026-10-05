//! Bounded adaptive Canvas curve geometry using the existing kurbo dependency.
//! Curves are transformed before flattening, so tolerance refers to bitmap
//! coordinates for the default path rather than unscaled author coordinates.

use super::*;
use kurbo::{Affine, Arc, BezPath, PathEl, Shape, flatten};
use serde::Deserialize;

const TOLERANCE: f64 = 0.025;
const MAX_COORDINATE: f64 = 16384.0;
const MAX_POINTS: usize = 8192;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    kind: String,
    #[serde(default)]
    points: Vec<[f64; 2]>,
    #[serde(default)]
    arc: Option<ArcRequest>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArcRequest {
    center: [f64; 2],
    radii: [f64; 2],
    start: f64,
    sweep: f64,
    rotation: f64,
    transform: [f64; 6],
}

pub(super) fn points(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(source)) = args.get(1) else {
        return JsValue::Null;
    };
    if source.len() > 2048 {
        return JsValue::Null;
    }
    serde_json::from_str::<Request>(source)
        .ok()
        .and_then(flatten_request)
        .map_or(JsValue::Null, |points| {
            JsValue::Array(
                points
                    .into_iter()
                    .map(|[x, y]| JsValue::Array(vec![JsValue::from(x), JsValue::from(y)]))
                    .collect(),
            )
        })
}

fn bounded(value: f64) -> bool {
    value.is_finite() && value.abs() <= MAX_COORDINATE
}

fn bounded_element(element: &PathEl) -> bool {
    let point = |p: kurbo::Point| bounded(p.x) && bounded(p.y);
    match *element {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => point(p),
        PathEl::QuadTo(a, b) => point(a) && point(b),
        PathEl::CurveTo(a, b, c) => point(a) && point(b) && point(c),
        PathEl::ClosePath => false,
    }
}

fn flatten_request(request: Request) -> Option<Vec<[f64; 2]>> {
    let mut path = BezPath::new();
    match (request.kind.as_str(), request.arc) {
        ("quadratic" | "cubic", None) => {
            let count = if request.kind == "cubic" { 4 } else { 3 };
            if request.points.len() != count
                || request.points.iter().flatten().any(|v| !bounded(*v))
            {
                return None;
            }
            let point = |index: usize| {
                kurbo::Point::new(request.points[index][0], request.points[index][1])
            };
            path.move_to(point(0));
            if count == 4 {
                path.curve_to(point(1), point(2), point(3));
            } else {
                path.quad_to(point(1), point(2));
            }
        }
        ("arc", Some(arc)) if request.points.is_empty() => {
            if arc
                .center
                .iter()
                .chain(&arc.radii)
                .chain(&arc.transform)
                .any(|v| !bounded(*v))
                || arc.radii.iter().any(|v| *v < 0.0)
                || !bounded(arc.start)
                || !bounded(arc.rotation)
                || !arc.sweep.is_finite()
                || arc.sweep.abs() > std::f64::consts::TAU
            {
                return None;
            }
            let shape = Arc::new(
                (arc.center[0], arc.center[1]),
                (arc.radii[0], arc.radii[1]),
                arc.start,
                arc.sweep,
                arc.rotation,
            );
            // Split the error budget between the upstream arc-to-cubic and
            // cubic-to-line stages. Bound intermediate controls before the
            // adaptive algorithm so a giant transform cannot expand its work.
            let matrix = Affine::new(arc.transform);
            let scale = arc.transform[..4]
                .iter()
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt()
                .max(1.0);
            let elements = shape
                .path_elements(TOLERANCE / (2.0 * scale))
                .take(65)
                .map(|el| matrix * el)
                .collect::<Vec<_>>();
            if elements.len() > 64 || elements.iter().any(|el| !bounded_element(el)) {
                return None;
            }
            path.extend(elements);
        }
        _ => return None,
    }
    let mut points = Vec::new();
    let mut invalid = false;
    // Fixed input coordinate/tolerance limits also bound kurbo's intermediate
    // subdivision work. Never retain more than the existing Canvas point cap.
    flatten(path, TOLERANCE / 2.0, |element| match element {
        PathEl::MoveTo(p) | PathEl::LineTo(p) if bounded(p.x) && bounded(p.y) => {
            if points.len() < MAX_POINTS {
                points.push([p.x, p.y]);
            } else {
                invalid = true;
            }
        }
        _ => invalid = true,
    });
    (!invalid).then_some(points)
}

#[cfg(test)]
mod tests;
