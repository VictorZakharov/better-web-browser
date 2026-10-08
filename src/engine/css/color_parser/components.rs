//! Color components are CSS tokens, not whitespace/comma-separated strings.
//! Nested math delimiters belong to the function; comments cannot invent units.
use super::super::{syntax::borrowed_components, value_parser};
use cssparser::{Parser, ParserInput, Token};

pub(super) fn legacy(body: &str) -> Option<bool> {
    Some(borrowed_components(body)?.contains(&","))
}

pub(super) fn components(body: &str, allow_legacy: bool) -> Option<([String; 3], Option<String>)> {
    let tokens = borrowed_components(body)?;
    let (channels, alpha) = if tokens.contains(&",") {
        if !allow_legacy
            || tokens
                .iter()
                .any(|token| token.eq_ignore_ascii_case("none"))
        {
            return None;
        }
        match tokens.as_slice() {
            [a, ",", b, ",", c] => ([*a, *b, *c], None),
            [a, ",", b, ",", c, ",", alpha] => ([*a, *b, *c], Some(*alpha)),
            _ => return None,
        }
    } else {
        match tokens.as_slice() {
            [a, b, c] => ([*a, *b, *c], None),
            [a, b, c, "/", alpha] => ([*a, *b, *c], Some(*alpha)),
            _ => return None,
        }
    };
    Some((channels.map(str::to_owned), alpha.map(str::to_owned)))
}

pub(super) fn percentage_kind(value: &str) -> bool {
    literal(value).is_some_and(|(_, percent)| percent)
        || value_parser::math_percentage(value).is_some()
}

pub(super) fn number_or_percent(value: &str, percent_scale: f64) -> Option<f64> {
    if value.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    if let Some((number, percent)) = literal(value) {
        return Some(if percent {
            number * percent_scale
        } else {
            number
        });
    }
    value_parser::math_number(value).map(f64::from).or_else(|| {
        value_parser::math_percentage(value).map(|number| f64::from(number) * percent_scale)
    })
}

fn literal(value: &str) -> Option<(f64, bool)> {
    let mut source = ParserInput::new(value);
    let mut parser = Parser::new(&mut source);
    let result = match parser.next().ok()? {
        Token::Number { value, .. } if value.is_finite() => (f64::from(*value), false),
        Token::Percentage { unit_value, .. } if unit_value.is_finite() => {
            (f64::from(*unit_value), true)
        }
        _ => return None,
    };
    parser.expect_exhausted().ok()?;
    Some(result)
}

pub(super) fn hue_literal(value: &str) -> Option<f64> {
    let mut source = ParserInput::new(value);
    let mut parser = Parser::new(&mut source);
    let degrees = match parser.next().ok()? {
        Token::Number { value, .. } if value.is_finite() => f64::from(*value),
        Token::Dimension { value, unit, .. } if value.is_finite() => {
            let number = f64::from(*value);
            match unit.to_ascii_lowercase().as_str() {
                "deg" => number,
                "grad" => number * 0.9,
                "rad" => number.to_degrees(),
                "turn" => number * 360.0,
                _ => return None,
            }
        }
        _ => return None,
    };
    parser.expect_exhausted().ok()?;
    Some(degrees.rem_euclid(360.0))
}
