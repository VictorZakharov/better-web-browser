//! Evaluation of the media features Breeze can truthfully report.

use super::{MediaEnvironment, Truth};

mod values;
use values::range_value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comparison {
    Less,
    LessEqual,
    Equal,
    GreaterEqual,
    Greater,
}

impl Comparison {
    fn compare(self, left: f32, right: f32) -> bool {
        match self {
            Self::Less => left < right,
            Self::LessEqual => left <= right,
            Self::Equal => (left - right).abs() < 0.001,
            Self::GreaterEqual => left >= right,
            Self::Greater => left > right,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Equal => "=",
            Self::GreaterEqual => ">=",
            Self::Greater => ">",
        }
    }
}

pub(super) fn evaluate(source: &str, environment: MediaEnvironment) -> Truth {
    let Some(source) = without_comments(source) else {
        return Truth::Unknown;
    };
    let source = source.trim();
    if source.is_empty() {
        return Truth::Unknown;
    }
    if let Some((name, value)) = source.split_once(':') {
        if value.contains(':') {
            return Truth::Unknown;
        }
        return plain(name.trim(), value.trim(), environment);
    }
    if let Some((segments, operators)) = comparisons(source) {
        return range(&segments, &operators, environment);
    }
    boolean(source, environment)
}

pub(super) fn canonical(source: &str) -> String {
    let source = without_comments(source).unwrap_or_default();
    let source = normalized_whitespace(&source);
    // Known values use CSSOM's canonical feature-name/keyword casing. Unknown
    // feature names and values are forward-compatible token streams: lowercasing
    // them would also alter quoted strings and custom function arguments.
    let known =
        evaluate(&source, MediaEnvironment::new(800.0, 600.0, 1.0, false)) != Truth::Unknown;
    let source = if known {
        source.to_ascii_lowercase()
    } else {
        source
    };
    if let Some((name, value)) = source.split_once(':') {
        let name = name.trim();
        let value = value.trim();
        let value = if known && is_aspect_ratio_name(name) {
            canonical_ratio(value)
        } else {
            value.to_string()
        };
        return format!("{name}: {value}");
    }
    if let Some((segments, operators)) = comparisons(&source) {
        let ratio_range = known && segments.iter().any(|segment| is_aspect_ratio_name(segment));
        let segment_text = |segment: &str| {
            if ratio_range && !is_aspect_ratio_name(segment) {
                canonical_ratio(segment)
            } else {
                segment.to_string()
            }
        };
        let mut result = segment_text(segments[0]);
        for (operator, segment) in operators.iter().zip(segments.iter().skip(1)) {
            result.push(' ');
            result.push_str(operator.text());
            result.push(' ');
            result.push_str(&segment_text(segment));
        }
        return result;
    }
    source
}

fn is_aspect_ratio_name(name: &str) -> bool {
    matches!(
        name.strip_prefix("min-")
            .or_else(|| name.strip_prefix("max-"))
            .unwrap_or(name),
        "aspect-ratio"
    )
}

fn canonical_ratio(value: &str) -> String {
    value
        .split_once('/')
        .map(|(numerator, denominator)| format!("{} / {}", numerator.trim(), denominator.trim()))
        .unwrap_or_else(|| value.to_string())
}

fn without_comments(input: &str) -> Option<String> {
    let mut result = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut quote = None;
    let mut escaped = false;
    while cursor < input.len() {
        if quote.is_none() && !escaped && input[cursor..].starts_with("/*") {
            let end = input[cursor + 2..].find("*/")?;
            result.push(' ');
            cursor += end + 4;
        } else {
            let character = input[cursor..].chars().next()?;
            result.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if quote == Some(character) {
                quote = None;
            } else if quote.is_none() && matches!(character, '\'' | '"') {
                quote = Some(character);
            }
            cursor += character.len_utf8();
        }
    }
    Some(result)
}

fn normalized_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut quote = None;
    let mut escaped = false;
    let mut pending_space = false;
    for character in input.chars() {
        if quote.is_none() && character.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !output.is_empty() {
            output.push(' ');
        }
        pending_space = false;
        output.push(character);
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && matches!(character, '\'' | '"') {
            quote = Some(character);
        }
    }
    output
}

fn boolean(name: &str, environment: MediaEnvironment) -> Truth {
    let name = name.trim().to_ascii_lowercase();
    let result = match name.as_str() {
        "width" | "device-width" => environment.viewport_width > 0.0,
        "height" | "device-height" => environment.viewport_height > 0.0,
        "resolution" => environment.resolution_dppx > 0.0,
        "aspect-ratio"
        | "orientation"
        | "hover"
        | "any-hover"
        | "pointer"
        | "any-pointer"
        | "prefers-color-scheme"
        | "display-mode"
        | "update"
        | "overflow-block"
        | "overflow-inline"
        | "color"
        | "color-gamut" => true,
        "prefers-reduced-motion"
        | "prefers-contrast"
        | "forced-colors"
        | "monochrome"
        | "color-index" => false,
        _ => return Truth::Unknown,
    };
    truth(result)
}

fn plain(name: &str, value: &str, environment: MediaEnvironment) -> Truth {
    let name = name.to_ascii_lowercase();
    let value = value.to_ascii_lowercase();
    if value.is_empty() {
        return Truth::Unknown;
    }
    if let Some(name) = name.strip_prefix("min-") {
        return range_feature(name, &value, Comparison::GreaterEqual, environment);
    }
    if let Some(name) = name.strip_prefix("max-") {
        return range_feature(name, &value, Comparison::LessEqual, environment);
    }
    if range_actual(&name, environment).is_some() {
        return range_feature(&name, &value, Comparison::Equal, environment);
    }
    let actual = match name.as_str() {
        "orientation" => {
            // MQ4 defines a square viewport as portrait: height >= width.
            if environment.viewport_width > environment.viewport_height {
                "landscape"
            } else {
                "portrait"
            }
        }
        "hover" | "any-hover" => "hover",
        "pointer" | "any-pointer" => "fine",
        "prefers-color-scheme" => {
            if environment.prefers_dark_color_scheme {
                "dark"
            } else {
                "light"
            }
        }
        "prefers-reduced-motion" | "prefers-contrast" => "no-preference",
        "forced-colors" => "none",
        "display-mode" => "browser",
        "update" => "fast",
        "overflow-block" | "overflow-inline" => "scroll",
        "color-gamut" => "srgb",
        _ => return Truth::Unknown,
    };
    let recognized = match name.as_str() {
        "orientation" => matches!(value.as_str(), "landscape" | "portrait"),
        "hover" | "any-hover" => matches!(value.as_str(), "none" | "hover"),
        "pointer" | "any-pointer" => matches!(value.as_str(), "none" | "coarse" | "fine"),
        "prefers-color-scheme" => matches!(value.as_str(), "dark" | "light"),
        "prefers-reduced-motion" => matches!(value.as_str(), "no-preference" | "reduce"),
        "prefers-contrast" => {
            matches!(value.as_str(), "no-preference" | "less" | "more" | "custom")
        }
        "forced-colors" => matches!(value.as_str(), "none" | "active"),
        "display-mode" => matches!(
            value.as_str(),
            "browser" | "fullscreen" | "standalone" | "minimal-ui" | "window-controls-overlay"
        ),
        "update" => matches!(value.as_str(), "none" | "slow" | "fast"),
        "overflow-block" => matches!(value.as_str(), "none" | "scroll" | "paged"),
        "overflow-inline" => matches!(value.as_str(), "none" | "scroll"),
        "color-gamut" => matches!(value.as_str(), "srgb" | "p3" | "rec2020"),
        _ => false,
    };
    if recognized {
        truth(value == actual)
    } else {
        Truth::Unknown
    }
}

fn range(segments: &[&str], operators: &[Comparison], environment: MediaEnvironment) -> Truth {
    match (segments, operators) {
        ([name, value], [operator]) if range_actual(name, environment).is_some() => {
            range_feature(name, value, *operator, environment)
        }
        ([value, name], [operator]) if range_actual(name, environment).is_some() => {
            let Some(actual) = range_actual(name, environment) else {
                return Truth::Unknown;
            };
            let Some(expected) = range_value(name, value, environment) else {
                return Truth::Unknown;
            };
            truth(operator.compare(expected, actual))
        }
        ([lower, name, upper], [first, second])
            if range_actual(name, environment).is_some() && same_direction(*first, *second) =>
        {
            let Some(actual) = range_actual(name, environment) else {
                return Truth::Unknown;
            };
            let (Some(low), Some(high)) = (
                range_value(name, lower, environment),
                range_value(name, upper, environment),
            ) else {
                return Truth::Unknown;
            };
            truth(first.compare(low, actual) && second.compare(actual, high))
        }
        _ => Truth::Unknown,
    }
}

fn same_direction(first: Comparison, second: Comparison) -> bool {
    (matches!(first, Comparison::Less | Comparison::LessEqual)
        && matches!(second, Comparison::Less | Comparison::LessEqual))
        || (matches!(first, Comparison::Greater | Comparison::GreaterEqual)
            && matches!(second, Comparison::Greater | Comparison::GreaterEqual))
}

fn range_feature(
    name: &str,
    value: &str,
    comparison: Comparison,
    environment: MediaEnvironment,
) -> Truth {
    let (Some(actual), Some(expected)) = (
        range_actual(name, environment),
        range_value(name, value, environment),
    ) else {
        return Truth::Unknown;
    };
    truth(comparison.compare(actual, expected))
}

fn range_actual(name: &str, environment: MediaEnvironment) -> Option<f32> {
    match name.trim().to_ascii_lowercase().as_str() {
        "width" | "device-width" => Some(environment.viewport_width),
        "height" | "device-height" => Some(environment.viewport_height),
        "resolution" => Some(environment.resolution_dppx),
        "aspect-ratio" => Some(environment.viewport_width / environment.viewport_height),
        "color" => Some(8.0), // 8-bit sRGB raster output per color component.
        "monochrome" | "color-index" => Some(0.0),
        _ => None,
    }
}

fn comparisons(input: &str) -> Option<(Vec<&str>, Vec<Comparison>)> {
    let mut segments = Vec::new();
    let mut operators = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    let mut depth = 0_u32;
    while cursor < input.len() {
        let character = input[cursor..].chars().next()?;
        match character {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            '<' | '>' | '=' if depth == 0 => {
                let segment = input[start..cursor].trim();
                if segment.is_empty() {
                    return None;
                }
                segments.push(segment);
                let equal = character != '=' && input.as_bytes().get(cursor + 1) == Some(&b'=');
                let operator = match (character, equal) {
                    ('<', false) => Comparison::Less,
                    ('<', true) => Comparison::LessEqual,
                    ('=', _) => Comparison::Equal,
                    ('>', false) => Comparison::Greater,
                    ('>', true) => Comparison::GreaterEqual,
                    _ => unreachable!(),
                };
                operators.push(operator);
                cursor += if equal { 2 } else { 1 };
                start = cursor;
                continue;
            }
            _ => {}
        }
        cursor += character.len_utf8();
    }
    if depth != 0 || operators.is_empty() || operators.len() > 2 {
        return None;
    }
    let last = input[start..].trim();
    if last.is_empty() {
        return None;
    }
    segments.push(last);
    Some((segments, operators))
}

fn truth(value: bool) -> Truth {
    if value { Truth::True } else { Truth::False }
}
