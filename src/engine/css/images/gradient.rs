//! CSS Images 3 gradient syntax. Parsing preserves positions until image sizing.
//! https://drafts.csswg.org/css-images-3/#gradients
use super::*;
pub(crate) mod geometry;
mod radial;
pub use radial::{Extent, RadialSize};

#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    Angle(f32),
    Corner { x: f32, y: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Linear(Direction),
    Radial {
        circle: bool,
        center: [Length; 2],
        size: RadialSize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    pub color: Option<Color>,
    pub position: Option<Length>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub shape: Shape,
    pub stops: Vec<Stop>,
    pub repeating: bool,
    source: String,
}

impl Gradient {
    pub(crate) fn parse(source: &str) -> Option<Self> {
        let (name, args) = function(source)?;
        let repeating = name.starts_with("repeating-");
        let linear = match name.trim_start_matches("repeating-") {
            "linear-gradient" => true,
            "radial-gradient" => false,
            _ => return None,
        };
        let mut parts = split_css_top_level(args, ',').collect::<Vec<_>>();
        if !(2..=128).contains(&parts.len()) {
            return None;
        }
        let default = if linear {
            Shape::Linear(Direction::Angle(180.0))
        } else {
            Shape::Radial {
                circle: false,
                center: [Length::Percent(50.0), Length::Percent(50.0)],
                size: RadialSize::Extent(Extent::FarthestCorner),
            }
        };
        let shape = if stop(parts[0]).is_none() {
            let prelude = parts.remove(0);
            if linear {
                Shape::Linear(direction(prelude)?)
            } else {
                radial::parse(prelude)?
            }
        } else {
            default
        };
        let mut stops = Vec::new();
        for part in parts {
            stops.extend(stop(part)?);
        }
        if stops.len() < 2 || stops.len() > 256 {
            return None;
        }
        Some(Self {
            shape,
            stops,
            repeating,
            source: source.trim().into(),
        })
    }
    pub(crate) fn css_text(&self) -> String {
        self.source.clone()
    }
}

fn direction(value: &str) -> Option<Direction> {
    if let Some(angle) = super::super::value_parser::math::radians(value) {
        return Some(Direction::Angle(angle.to_degrees()));
    }
    let parts = borrowed_components(value)?;
    if parts.first().and_then(|part| ident(part)).as_deref() != Some("to") {
        return None;
    }
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let (mut x, mut y) = (0.0, 0.0);
    for part in &parts[1..] {
        match ident(part)?.as_str() {
            "left" if x == 0.0 => x = -1.0,
            "right" if x == 0.0 => x = 1.0,
            "top" if y == 0.0 => y = -1.0,
            "bottom" if y == 0.0 => y = 1.0,
            _ => return None,
        }
    }
    Some(Direction::Corner { x, y })
}

impl Gradient {
    pub(in crate::engine::css) fn resolve_relative_units(
        &mut self,
        width: f32,
        height: f32,
        root_font: f32,
    ) {
        let resolve = |value: &mut Length| {
            *value = value
                .clone()
                .resolve_root_font_units(root_font)
                .resolve_viewport_units(width, height)
        };
        for stop in &mut self.stops {
            if let Some(position) = &mut stop.position {
                resolve(position);
            }
        }
        if let Shape::Radial { center, size, .. } = &mut self.shape {
            center.iter_mut().for_each(resolve);
            if let RadialSize::Explicit(lengths) = size {
                lengths.iter_mut().for_each(resolve);
            }
        }
    }
}

fn stop(value: &str) -> Option<Vec<Stop>> {
    let parts = borrowed_components(value)?;
    if !(1..=3).contains(&parts.len()) {
        return None;
    }
    let color = if ident(parts[0]).as_deref() == Some("currentcolor") {
        None
    } else {
        Some(parse_color(parts[0])?)
    };
    if parts.len() == 1 {
        return Some(vec![Stop {
            color,
            position: None,
        }]);
    }
    parts[1..]
        .iter()
        .map(|value| {
            let position = parse_length(value)?;
            (position != Length::Auto).then_some(Stop {
                color,
                position: Some(position),
            })
        })
        .collect()
}
