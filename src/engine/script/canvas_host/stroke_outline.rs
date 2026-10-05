//! Shared bounded stroke geometry for painting and exact curve hit testing.
//! Dashing/caps/joins reuse tiny-skia; kurbo evaluates the resulting outline,
//! not a bitmap approximation or a second independently dashed centerline.

use super::path::{MAX_COORDINATE, MAX_POINTS, Request};
use kurbo::{BezPath, ParamCurveNearest, Shape};
use resvg::tiny_skia::{
    LineCap, LineJoin, Path, PathBuilder, PathSegment, Stroke, StrokeDash, Transform,
};

const MAX_DASH_ENTRIES: usize = 1024;
const MAX_DASH_TRANSITIONS: f64 = 65536.0;
const MAX_OUTLINE_POINTS: usize = 262144;

pub(super) enum Outline {
    Empty,
    Path(Path),
}

pub(super) fn build(request: &Request) -> Option<Outline> {
    if !request.line_width.is_finite()
        || !(0.0..=4096.0).contains(&request.line_width)
        || request.line_width == 0.0
        || !request.miter_limit.is_finite()
        || !(1.0..=64.0).contains(&request.miter_limit)
        || request.parts.len() > MAX_POINTS
        || request
            .transform
            .iter()
            .any(|value| !value.is_finite() || value.abs() > MAX_COORDINATE)
        || request.dash.len() > MAX_DASH_ENTRIES
        || !request.dash.len().is_multiple_of(2)
        || request
            .dash
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        || !request.dash_offset.is_finite()
    {
        return None;
    }
    let mut builder = PathBuilder::new();
    let mut point_count = 0usize;
    let mut length = 0.0f64;
    for part in &request.parts {
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
        // HTML tracing prunes zero-length lines and ignores point-only subpaths.
        if !part.points.iter().any(|point| point != first) {
            continue;
        }
        builder.move_to(first[0], first[1]);
        let mut previous = first;
        for point in part.points.iter().skip(1) {
            if point == previous {
                continue;
            }
            length += f64::from(point[0] - previous[0]).hypot(f64::from(point[1] - previous[1]));
            builder.line_to(point[0], point[1]);
            previous = point;
        }
        if part.closed {
            length += f64::from(first[0] - previous[0]).hypot(f64::from(first[1] - previous[1]));
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
    let resolution = a.hypot(b).max(c.hypot(d)).clamp(1.0, 256.0);
    let Some(mut path) = builder.finish() else {
        return Some(Outline::Empty);
    };
    let period: f64 = request.dash.iter().sum();
    if !period.is_finite() {
        return None;
    }
    if period > 0.0 {
        // Bound expansion BEFORE upstream allocates. Include phase resets and
        // zero entries in the transition bound, not just the number of on runs.
        let transitions =
            (length / period + request.parts.len() as f64) * request.dash.len() as f64;
        if transitions > MAX_DASH_TRANSITIONS || period > f64::from(f32::MAX) {
            return None;
        }
        let phase = request.dash_offset.rem_euclid(period) as f32;
        let values: Vec<f32> = request.dash.iter().map(|value| *value as f32).collect();
        if values
            .iter()
            .zip(&request.dash)
            .any(|(narrow, wide)| *wide > 0.0 && *narrow == 0.0)
        {
            return None; // Underflow must not turn a positive pattern into a solid stroke.
        }
        let dash = StrokeDash::new(values, phase)?;
        let Some(dashed) = path.dash(&dash, resolution) else {
            // A valid pattern can put an entire short contour in an off run.
            // Preserve an empty successful result, not the unsupported sentinel.
            return Some(Outline::Empty);
        };
        path = dashed;
    }
    // The all-zero pattern is solid, not an invalid/empty stroke.
    // Path::stroke does not consume Stroke::dash, so dash the centerline first.
    let Some(outline) = path.stroke(&stroke, resolution) else {
        return Some(Outline::Empty);
    };
    let outline = outline.transform(transform)?;
    (outline.points().len() <= MAX_OUTLINE_POINTS).then_some(Outline::Path(outline))
}

pub(super) fn contains(outline: &Outline, point: (f64, f64)) -> bool {
    let Outline::Path(outline) = outline else {
        return false;
    };
    let mut path = BezPath::new();
    let convert = |p: resvg::tiny_skia::Point| kurbo::Point::new(f64::from(p.x), f64::from(p.y));
    for segment in outline.segments() {
        match segment {
            PathSegment::MoveTo(p) => path.move_to(convert(p)),
            PathSegment::LineTo(p) => path.line_to(convert(p)),
            PathSegment::QuadTo(a, b) => path.quad_to(convert(a), convert(b)),
            PathSegment::CubicTo(a, b, c) => path.curve_to(convert(a), convert(b), convert(c)),
            PathSegment::Close => path.close_path(),
        }
    }
    let point = kurbo::Point::new(point.0, point.1);
    path.contains(point)
        || path.segments().any(|segment| {
            // Canvas includes points on the boundary. Use curve distance rather
            // than perturbing the query or expanding the stroke by a visible pixel.
            segment.nearest(point, 1e-8).distance_sq <= 1e-14
        })
}

#[cfg(test)]
#[path = "stroke_outline/tests.rs"]
mod tests;
