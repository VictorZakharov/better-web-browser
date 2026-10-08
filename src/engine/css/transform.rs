//! Parsing and resolution for the implemented 2D translation transform subset.

use super::*;
mod interpolation;
mod parser;
pub(crate) use interpolation::interpolate;
pub(super) use parser::parse_transform;

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct TransformList(Vec<TranslateOperation>);

#[derive(Debug, Clone, PartialEq)]
struct TranslateOperation {
    x: Length,
    y: Length,
}

impl TransformList {
    pub(super) fn resolve_root_font_units(&mut self, root_font_size: f32) {
        for operation in &mut self.0 {
            operation.x = operation.x.clone().resolve_root_font_units(root_font_size);
            operation.y = operation.y.clone().resolve_root_font_units(root_font_size);
        }
    }

    pub(crate) fn resolve(&self, width: f32, height: f32, font_size: f32) -> (f32, f32) {
        self.0.iter().fold((0.0, 0.0), |(x, y), operation| {
            (
                x + operation.x.resolve(width, font_size).unwrap_or(0.0),
                y + operation.y.resolve(height, font_size).unwrap_or(0.0),
            )
        })
    }

    pub(crate) fn is_none(&self) -> bool {
        self.0.is_empty()
    }
}

pub(super) fn serialize_transform(transform: &TransformList) -> String {
    if transform.is_none() {
        return "none".into();
    }
    transform
        .0
        .iter()
        .map(|operation| {
            format!(
                "translate({}, {})",
                serialize_length(operation.x.clone()),
                serialize_length(operation.y.clone())
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn serialize_length(length: Length) -> String {
    match length {
        Length::Math(value) => value.css_text(),
        Length::Px(value) => format!("{value}px"),
        Length::Percent(value) => format!("{value}%"),
        Length::Em(value) => format!("{value}em"),
        Length::Rem(value) => format!("{value}rem"),
        Length::Vw(value) => format!("{value}vw"),
        Length::Vh(value) => format!("{value}vh"),
        Length::Vmin(value) => format!("{value}vmin"),
        Length::Vmax(value) => format!("{value}vmax"),
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } => {
            let mut expression = String::new();
            for (value, unit) in [
                (px, "px"),
                (percent, "%"),
                (em, "em"),
                (rem, "rem"),
                (vw, "vw"),
                (vh, "vh"),
                (vmin, "vmin"),
                (vmax, "vmax"),
            ] {
                if value.abs() <= f32::EPSILON {
                    continue;
                }
                if expression.is_empty() {
                    expression.push_str(&format!("{value}{unit}"));
                } else if value.is_sign_negative() {
                    expression.push_str(&format!(" - {}{unit}", value.abs()));
                } else {
                    expression.push_str(&format!(" + {value}{unit}"));
                }
            }
            if expression.is_empty() {
                "0px".into()
            } else {
                format!("calc({expression})")
            }
        }
        Length::Auto => "0px".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lists_of_two_dimensional_translations() {
        let transform = parse_transform("translateX(10px) translateY(-50%)").unwrap();
        assert_eq!(transform.resolve(200.0, 80.0, 16.0), (10.0, -40.0));
        let calculated = parse_transform("translate(calc(10px - 25%), 2em)").unwrap();
        assert_eq!(
            serialize_transform(&calculated),
            "translate(calc(10px - 25%), 2em)"
        );
        assert!(parse_transform("rotate(10deg)").is_none());
        assert!(parse_transform("matrix(2, 0, 0, 2, 0, 0)").is_none());
    }
}
