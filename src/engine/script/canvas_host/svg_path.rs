//! A bounded adapter over the already-locked svgtypes SVG 2 path parser.
//!
//! Keep relative coordinates and original arc parameters for the existing
//! Canvas geometry backend. Parsing is not rasterization: converting arcs to
//! another provider's cubic approximation would alter established pixels.

use super::*;
use svgtypes::{PathParser, PathSegment};

const MAX_SOURCE_BYTES: usize = 131_072;
const MAX_SEGMENTS: usize = 8192;

pub(super) fn segments(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(source)) = args.get(1) else {
        return JsValue::Null;
    };
    parse(source).map_or(JsValue::Null, |segments| {
        JsValue::Array(
            segments
                .into_iter()
                .map(|(command, coordinates)| {
                    let mut segment = Vec::with_capacity(coordinates.len() + 1);
                    segment.push(JsValue::String(command.to_string()));
                    segment.extend(coordinates.into_iter().map(JsValue::from));
                    JsValue::Array(segment)
                })
                .collect(),
        )
    })
}

// Invalid data terminates at the last completed segment (SVG 2 error handling).
// Admission/resource failures return None instead, never a truncated success.
fn parse(source: &str) -> Option<Vec<(char, Vec<f64>)>> {
    if source.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let mut output = Vec::new();
    for segment in PathParser::from(source) {
        let Ok(segment) = segment else { break };
        if output.len() == MAX_SEGMENTS {
            return None;
        }
        let command = char::from(segment.command());
        let coordinates = coordinates(segment);
        if coordinates.iter().any(|value| !value.is_finite()) {
            break;
        }
        output.push((command, coordinates));
    }
    Some(output)
}

fn coordinates(segment: PathSegment) -> Vec<f64> {
    match segment {
        PathSegment::MoveTo { x, y, .. }
        | PathSegment::LineTo { x, y, .. }
        | PathSegment::SmoothQuadratic { x, y, .. } => vec![x, y],
        PathSegment::HorizontalLineTo { x, .. } => vec![x],
        PathSegment::VerticalLineTo { y, .. } => vec![y],
        PathSegment::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
            ..
        } => vec![x1, y1, x2, y2, x, y],
        PathSegment::SmoothCurveTo { x2, y2, x, y, .. } => vec![x2, y2, x, y],
        PathSegment::Quadratic { x1, y1, x, y, .. } => vec![x1, y1, x, y],
        PathSegment::EllipticalArc {
            rx,
            ry,
            x_axis_rotation,
            large_arc,
            sweep,
            x,
            y,
            ..
        } => vec![
            rx,
            ry,
            x_axis_rotation,
            f64::from(u8::from(large_arc)),
            f64::from(u8::from(sweep)),
            x,
            y,
        ],
        PathSegment::ClosePath { .. } => vec![],
    }
}

#[cfg(test)]
mod tests;
