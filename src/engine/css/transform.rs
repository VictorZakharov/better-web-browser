//! Deferred CSS 2D transforms. Layout supplies the reference box before matrix composition.

use super::*;
pub(crate) mod individual;
pub(crate) mod individual_linear;
mod interpolation;
pub(crate) mod origin;
mod parser;
pub(super) mod properties;
mod serialization;
pub(in crate::engine::css) mod svg_attribute;
pub(crate) use interpolation::interpolate;
pub(super) use parser::parse_transform;

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct TransformList(Vec<Operation>);

#[derive(Debug, Clone, PartialEq)]
enum Operation {
    Translate(TranslateOperation),
    Scale([f32; 2]),
    Rotate(f32),
    Skew([f32; 2]),
    Matrix(crate::engine::affine::Affine2D),
}

#[derive(Debug, Clone, PartialEq)]
struct TranslateOperation {
    x: Length,
    y: Length,
}

impl TransformList {
    pub(crate) fn apply_zoom(&mut self, scale: f32) {
        for operation in &mut self.0 {
            match operation {
                Operation::Translate(v) => {
                    v.x = values::zoom::length(&v.x, scale);
                    v.y = values::zoom::length(&v.y, scale);
                }
                Operation::Matrix(v) => {
                    v.0[4] *= f64::from(scale);
                    v.0[5] *= f64::from(scale);
                }
                _ => {}
            }
        }
    }
    pub(super) fn resolve_root_font_units(&mut self, root_font_size: f32) {
        for operation in &mut self.0 {
            if let Operation::Translate(v) = operation {
                v.x = v.x.clone().resolve_root_font_units(root_font_size);
                v.y = v.y.clone().resolve_root_font_units(root_font_size);
            }
        }
    }

    pub(crate) fn resolve_viewport_units(&mut self, width: f32, height: f32) {
        for operation in &mut self.0 {
            if let Operation::Translate(v) = operation {
                v.x = v.x.clone().resolve_viewport_units(width, height);
                v.y = v.y.clone().resolve_viewport_units(width, height);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn resolve(&self, width: f32, height: f32, font_size: f32) -> (f32, f32) {
        let matrix = self.matrix(width, height, font_size);
        (matrix.0[4] as f32, matrix.0[5] as f32)
    }

    pub(crate) fn matrix(
        &self,
        width: f32,
        height: f32,
        font: f32,
    ) -> crate::engine::affine::Affine2D {
        use crate::engine::affine::Affine2D;
        self.0.iter().fold(Affine2D::IDENTITY, |matrix, operation| {
            let next = match operation {
                Operation::Translate(v) => Affine2D::translate(
                    f64::from(v.x.resolve(width, font).unwrap_or(0.0)),
                    f64::from(v.y.resolve(height, font).unwrap_or(0.0)),
                ),
                Operation::Scale([x, y]) => {
                    Affine2D([f64::from(*x), 0.0, 0.0, f64::from(*y), 0.0, 0.0])
                }
                Operation::Rotate(angle) => {
                    let (sin, cos) = f64::from(*angle).sin_cos();
                    Affine2D([cos, sin, -sin, cos, 0.0, 0.0])
                }
                Operation::Skew([x, y]) => {
                    Affine2D([1.0, f64::from(*y).tan(), f64::from(*x).tan(), 1.0, 0.0, 0.0])
                }
                Operation::Matrix(v) => *v,
            };
            matrix.then_local(next)
        })
    }

    pub(crate) fn is_none(&self) -> bool {
        self.0.is_empty()
    }
}

pub(super) use serialization::serialize_transform;

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
        assert!(parse_transform("rotate(10deg)").is_some());
        assert!(parse_transform("matrix(2, 0, 0, 2, 0, 0)").is_some());
    }
}
