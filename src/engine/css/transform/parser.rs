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
            ("translate", [x]) | ("translatex", [x]) => TranslateOperation {
                x: length(x)?,
                y: zero,
            },
            ("translate", [x, ",", y]) => TranslateOperation {
                x: length(x)?,
                y: length(y)?,
            },
            ("translatey", [y]) => TranslateOperation {
                x: zero,
                y: length(y)?,
            },
            ("matrix", [a, ",", b, ",", c, ",", d, ",", x, ",", y]) => {
                let [Some(a), Some(b), Some(c), Some(d), Some(x), Some(y)] =
                    [*a, *b, *c, *d, *x, *y].map(numbers::number)
                else {
                    return None;
                };
                // The painter implements translations only. Almost-identity
                // rotation/scale coefficients cannot be admitted by epsilon.
                if a != 1.0 || b != 0.0 || c != 0.0 || d != 1.0 {
                    return None;
                }
                TranslateOperation {
                    x: Length::Px(x),
                    y: Length::Px(y),
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

#[cfg(test)]
mod tests;
