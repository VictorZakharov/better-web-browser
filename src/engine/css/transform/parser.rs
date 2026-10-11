//! CSS Transforms 1 §9.1 requires commas between translate/matrix arguments.
//! Tokenized groups preserve nested math, escapes and comments. Invalid second
//! coordinates must invalidate the whole declaration, not silently become zero.
use super::*;
use crate::engine::css::{
    syntax::{borrowed_components, function},
    value_parser::numbers,
};

pub(in crate::engine::css) fn parse_transform(value: &str) -> Option<TransformList> {
    let value = value.trim();
    if value.len() > 16_384 {
        return None;
    }
    if value.eq_ignore_ascii_case("none") {
        return Some(TransformList::default());
    }
    let functions = borrowed_components(value)?;
    if functions.is_empty() || functions.len() > 64 {
        return None;
    }
    let mut operations = Vec::with_capacity(functions.len());
    for component in functions {
        let (name, body) = function(component)?;
        let arguments = borrowed_components(body)?;
        let zero = Length::Px(0.0);
        let operation = match (name.to_ascii_lowercase().as_str(), arguments.as_slice()) {
            ("translate", [x]) | ("translatex", [x]) => Operation::Translate(TranslateOperation {
                x: length(x)?,
                y: zero,
            }),
            ("translate", [x, ",", y]) => Operation::Translate(TranslateOperation {
                x: length(x)?,
                y: length(y)?,
            }),
            ("translatey", [y]) => Operation::Translate(TranslateOperation {
                x: zero,
                y: length(y)?,
            }),
            ("scale", [v]) => {
                let v = scale(v)?;
                Operation::Scale([v, v])
            }
            ("scale", [x, ",", y]) => Operation::Scale([scale(x)?, scale(y)?]),
            ("scalex", [x]) => Operation::Scale([scale(x)?, 1.0]),
            ("scaley", [y]) => Operation::Scale([1.0, scale(y)?]),
            ("rotate", [v]) => Operation::Rotate(angle(v)?),
            ("skew", [x]) | ("skewx", [x]) => Operation::Skew([angle(x)?, 0.0]),
            ("skew", [x, ",", y]) => Operation::Skew([angle(x)?, angle(y)?]),
            ("skewy", [y]) => Operation::Skew([0.0, angle(y)?]),
            ("matrix", [a, ",", b, ",", c, ",", d, ",", x, ",", y]) => {
                let [Some(a), Some(b), Some(c), Some(d), Some(x), Some(y)] =
                    [*a, *b, *c, *d, *x, *y].map(numbers::number)
                else {
                    return None;
                };
                // Keep identity-linear matrices as translations so percentages in
                // interpolation stay deferred rather than taking a matrix fallback.
                if [a, b, c, d] == [1.0, 0.0, 0.0, 1.0] {
                    Operation::Translate(TranslateOperation {
                        x: Length::Px(x),
                        y: Length::Px(y),
                    })
                } else {
                    Operation::Matrix(crate::engine::affine::Affine2D(
                        [a, b, c, d, x, y].map(f64::from),
                    ))
                }
            }
            _ => return None,
        };
        operations.push(operation);
    }
    Some(TransformList(operations))
}

fn length(value: &str) -> Option<Length> {
    let length = parse_length(value)?;
    (!matches!(length, Length::Auto)).then_some(length)
}

pub(super) fn scale(value: &str) -> Option<f32> {
    numbers::number(value).or_else(|| numbers::percentage(value).map(|v| v / 100.0))
}

pub(super) fn angle(value: &str) -> Option<f32> {
    // Unitless zero is valid in transform functions, but not other numbers.
    super::super::value_parser::math::radians(value)
        .or_else(|| (numbers::number(value) == Some(0.0)).then_some(0.0))
}

#[cfg(test)]
mod tests;
