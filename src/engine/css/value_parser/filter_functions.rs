//! Filter Effects function admission shared by CSS-value consumers, using the tokenizer,
//! typed math, length and color parsers rather than a second regexp grammar.
//! https://drafts.csswg.org/filter-effects-1/#filter-functions
use super::*;

const MAX_SOURCE: usize = 16_384;
const MAX_FUNCTIONS: usize = 64;

#[derive(Clone, Copy)]
pub(crate) struct Environment {
    pub font_size: f32,
    pub root_font_size: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub current_color: Color,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            font_size: 10.0,
            root_font_size: 16.0,
            viewport_width: 300.0,
            viewport_height: 150.0,
            current_color: Color::BLACK,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Operation {
    pub name: &'static str,
    pub value: Argument,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Argument {
    Scalar(f32),
    Shadow {
        x: f32,
        y: f32,
        blur: f32,
        channels: [u8; 4],
        origin_clean: bool,
    },
}

pub(crate) fn parse(source: &str, environment: Environment) -> Option<Vec<Operation>> {
    if source.len() > MAX_SOURCE || source.trim().is_empty() {
        return None;
    }
    if syntax::ident(source).as_deref() == Some("none") {
        return Some(Vec::new());
    }
    let parts = syntax::borrowed_components(source)?;
    if parts.is_empty() || parts.len() > MAX_FUNCTIONS {
        return None;
    }
    parts
        .into_iter()
        .map(|part| {
            let (name, argument) = syntax::function(part)?;
            let argument = if syntax::borrowed_components(argument)?.is_empty() {
                ""
            } else {
                argument
            };
            let name = match name.as_str() {
                "blur" => "blur",
                "brightness" => "brightness",
                "contrast" => "contrast",
                "drop-shadow" => "drop-shadow",
                "grayscale" => "grayscale",
                "hue-rotate" => "hue-rotate",
                "invert" => "invert",
                "opacity" => "opacity",
                "saturate" => "saturate",
                "sepia" => "sepia",
                _ => return None,
            };
            let value = match name {
                "drop-shadow" => shadow(argument, environment)?,
                "blur" => Argument::Scalar(if argument.trim().is_empty() {
                    0.0
                } else {
                    nonnegative_length(argument, environment)?
                }),
                "hue-rotate" => Argument::Scalar(
                    if argument.trim().is_empty()
                        || (syntax::function(argument).is_none()
                            && value_parser::numbers::number(argument) == Some(0.0))
                    {
                        0.0
                    } else {
                        value_parser::math::radians(argument)?
                    },
                ),
                _ => Argument::Scalar(factor(argument, name)?),
            };
            Some(Operation { name, value })
        })
        .collect()
}

fn factor(source: &str, name: &str) -> Option<f32> {
    let source = source.trim();
    let amount = if source.is_empty() {
        1.0
    } else {
        value_parser::numbers::number(source)
            .or_else(|| value_parser::numbers::percentage(source).map(|value| value / 100.0))?
    };
    let amount = nonnegative(source, amount)?;
    Some(
        if matches!(name, "grayscale" | "invert" | "opacity" | "sepia") {
            amount.min(1.0)
        } else {
            amount
        },
    )
}

fn length(source: &str, environment: Environment) -> Option<f32> {
    let value = parse_length(source)?;
    // Percentage cancellation still carries percentage type in the typed math
    // parser. Filter blur/offset grammar is <length>, never <length-percentage>.
    if value.has_percentage() || matches!(value, Length::Auto) {
        return None;
    }
    let value = value
        .resolve_root_font_units(environment.root_font_size)
        .resolve_viewport_units(environment.viewport_width, environment.viewport_height)
        .resolve(0.0, environment.font_size)?;
    value.is_finite().then_some(value)
}

fn nonnegative(source: &str, value: f32) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    // CSS Values 4 range checking clamps a calculation's computed result;
    // negative literal values instead invalidate the declaration.
    if syntax::function(source).is_some() {
        Some(value.max(0.0))
    } else {
        (value >= 0.0).then_some(value)
    }
}

fn nonnegative_length(source: &str, environment: Environment) -> Option<f32> {
    nonnegative(source, length(source, environment)?)
}

fn shadow(source: &str, environment: Environment) -> Option<Argument> {
    let mut parts = syntax::borrowed_components(source)?;
    if !(2..=4).contains(&parts.len()) {
        return None;
    }
    let color = |source: &str| {
        if syntax::ident(source).as_deref() == Some("currentcolor") {
            Some(environment.current_color)
        } else {
            parse_color(source)
        }
    };
    let (channels, origin_clean) = if length(parts[0], environment).is_none() {
        let source = parts.remove(0);
        let result = color(source)?;
        (
            [result.red, result.green, result.blue, result.alpha],
            syntax::ident(source).as_deref() != Some("currentcolor"),
        )
    } else if length(parts.last()?, environment).is_none() {
        let source = parts.pop()?;
        let result = color(source)?;
        (
            [result.red, result.green, result.blue, result.alpha],
            syntax::ident(source).as_deref() != Some("currentcolor"),
        )
    } else {
        let result = environment.current_color;
        ([result.red, result.green, result.blue, result.alpha], false)
    };
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    Some(Argument::Shadow {
        x: length(parts[0], environment)?,
        y: length(parts[1], environment)?,
        blur: if parts.len() == 3 {
            nonnegative_length(parts[2], environment)?
        } else {
            0.0
        },
        channels,
        origin_clean,
    })
}

#[cfg(test)]
mod tests;
