//! Parsing and resolution for the implemented 2D translation transform subset.

use super::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct TransformList(Vec<TranslateOperation>);

#[derive(Debug, Clone, Copy, PartialEq)]
struct TranslateOperation {
    x: Length,
    y: Length,
}

impl TransformList {
    pub(super) fn resolve_root_font_units(&mut self, root_font_size: f32) {
        for operation in &mut self.0 {
            operation.x = operation.x.resolve_root_font_units(root_font_size);
            operation.y = operation.y.resolve_root_font_units(root_font_size);
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

pub(super) fn parse_transform(value: &str) -> Option<TransformList> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(TransformList::default());
    }
    let mut rest = value;
    let mut operations = Vec::new();
    while !rest.trim_start().is_empty() {
        rest = rest.trim_start();
        let open = rest.find('(')?;
        let name = rest[..open].trim().to_ascii_lowercase();
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return None;
        }
        let close = matching_parenthesis(rest, open)?;
        let arguments = split_arguments(&rest[open + 1..close]);
        let operation = match name.as_str() {
            "translate" if (1..=2).contains(&arguments.len()) => TranslateOperation {
                x: parse_length(arguments[0])?,
                y: arguments
                    .get(1)
                    .and_then(|value| parse_length(value))
                    .unwrap_or(Length::Px(0.0)),
            },
            "translatex" if arguments.len() == 1 => TranslateOperation {
                x: parse_length(arguments[0])?,
                y: Length::Px(0.0),
            },
            "translatey" if arguments.len() == 1 => TranslateOperation {
                x: Length::Px(0.0),
                y: parse_length(arguments[0])?,
            },
            "matrix" if arguments.len() == 6 => {
                let values = arguments
                    .iter()
                    .map(|argument| {
                        argument
                            .parse::<f32>()
                            .ok()
                            .filter(|value| value.is_finite())
                    })
                    .collect::<Option<Vec<_>>>()?;
                if (values[0] - 1.0).abs() > f32::EPSILON
                    || values[1].abs() > f32::EPSILON
                    || values[2].abs() > f32::EPSILON
                    || (values[3] - 1.0).abs() > f32::EPSILON
                {
                    return None;
                }
                TranslateOperation {
                    x: Length::Px(values[4]),
                    y: Length::Px(values[5]),
                }
            }
            _ => return None,
        };
        operations.push(operation);
        rest = &rest[close + 1..];
    }
    (!operations.is_empty()).then_some(TransformList(operations))
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
                serialize_length(operation.x),
                serialize_length(operation.y)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn matching_parenthesis(value: &str, open: usize) -> Option<usize> {
    let mut depth = 0_u32;
    for (offset, character) in value[open..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_arguments(value: &str) -> Vec<&str> {
    let comma_separated = split_css_top_level(value, ',').collect::<Vec<_>>();
    if comma_separated.len() > 1 {
        return comma_separated
            .into_iter()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect();
    }
    let mut depth = 0_i32;
    let mut start = None;
    let mut arguments = Vec::new();
    for (index, character) in value.char_indices() {
        match character {
            '(' => {
                depth += 1;
                start.get_or_insert(index);
            }
            ')' => depth = (depth - 1).max(0),
            character if character.is_whitespace() && depth == 0 => {
                if let Some(argument_start) = start.take() {
                    arguments.push(value[argument_start..index].trim());
                }
            }
            _ => {
                start.get_or_insert(index);
            }
        }
    }
    if let Some(argument_start) = start {
        arguments.push(value[argument_start..].trim());
    }
    arguments
}

fn serialize_length(length: Length) -> String {
    match length {
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
