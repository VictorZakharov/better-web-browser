//! CSS Images 3 §3.2: explicit radial sizes, extents, and their degenerate limits.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Extent {
    ClosestSide,
    FarthestSide,
    ClosestCorner,
    FarthestCorner,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RadialSize {
    Extent(Extent),
    Explicit([Length; 2]),
}

pub(super) fn parse(value: &str) -> Option<Shape> {
    let parts = borrowed_components(value)?;
    let at = parts.iter().position(|v| ident(v).as_deref() == Some("at"));
    let head = &parts[..at.unwrap_or(parts.len())];
    let (mut shape, mut extent, mut lengths) = (None, None, Vec::new());
    for part in head {
        match ident(part).as_deref() {
            Some("circle" | "ellipse") if shape.is_none() => {
                shape = Some(ident(part).as_deref() == Some("circle"));
            }
            Some("closest-side" | "farthest-side" | "closest-corner" | "farthest-corner")
                if extent.is_none() =>
            {
                extent = Some(match ident(part)?.as_str() {
                    "closest-side" => Extent::ClosestSide,
                    "farthest-side" => Extent::FarthestSide,
                    "closest-corner" => Extent::ClosestCorner,
                    _ => Extent::FarthestCorner,
                });
            }
            _ => {
                let length = parse_length(part)?;
                if length == Length::Auto
                    || lengths.len() == 2
                    || (function(part).is_none()
                        && length.resolve(100.0, 16.0).is_none_or(|v| v < 0.0))
                {
                    return None;
                }
                lengths.push(length);
            }
        }
    }
    if extent.is_some() && !lengths.is_empty() {
        return None;
    }
    let circle = shape.unwrap_or(lengths.len() == 1);
    let size = match lengths.as_slice() {
        [] => RadialSize::Extent(extent.unwrap_or(Extent::FarthestCorner)),
        [radius] if circle && !radius.has_percentage() => {
            RadialSize::Explicit([radius.clone(), radius.clone()])
        }
        [x, y] if !circle => RadialSize::Explicit([x.clone(), y.clone()]),
        _ => return None,
    };
    let center = if let Some(at) = at {
        let (x, y) = parse_background_position(&parts[at + 1..].join(" "))?;
        [x, y]
    } else {
        [Length::Percent(50.0), Length::Percent(50.0)]
    };
    Some(Shape::Radial {
        circle,
        center,
        size,
    })
}

impl RadialSize {
    pub(super) fn resolve(
        &self,
        dimensions: [f32; 2],
        center: [f32; 2],
        font: f32,
        zoom: f32,
        circle: bool,
    ) -> Option<([f32; 2], bool)> {
        let mut radii = match self {
            Self::Explicit(values) => [
                zoom::length(&values[0], zoom)
                    .resolve(dimensions[0], font)?
                    .max(0.0),
                zoom::length(&values[1], zoom)
                    .resolve(dimensions[1], font)?
                    .max(0.0),
            ],
            Self::Extent(extent) => {
                let closest = matches!(extent, Extent::ClosestSide | Extent::ClosestCorner);
                let sides = [0, 1].map(|axis| {
                    let a = center[axis].abs();
                    let b = (dimensions[axis] - center[axis]).abs();
                    if closest { a.min(b) } else { a.max(b) }
                });
                let corner = matches!(extent, Extent::ClosestCorner | Extent::FarthestCorner);
                if circle {
                    let radius = if corner {
                        sides[0].hypot(sides[1])
                    } else if closest {
                        sides[0].min(sides[1])
                    } else {
                        sides[0].max(sides[1])
                    };
                    [radius; 2]
                } else if corner {
                    sides.map(|v| v * std::f32::consts::SQRT_2)
                } else {
                    sides
                }
            }
        };
        if radii.iter().chain(center.iter()).any(|v| !v.is_finite()) {
            return None;
        }
        let flat = !circle && radii[0] > 0.0 && radii[1] == 0.0;
        // The spec explicitly permits arbitrary very small/large radii for
        // these limits. Keep shader inputs finite; no large bitmap is allocated.
        if circle && radii[0] == 0.0 {
            radii = [0.000_001; 2];
        } else if radii[0] == 0.0 {
            radii = [0.000_001, 1_000_000.0];
        }
        Some((radii, flat))
    }
}

#[cfg(test)]
mod tests;
