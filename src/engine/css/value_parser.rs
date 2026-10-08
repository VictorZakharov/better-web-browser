//! Length, calc(), and color parsing.

pub(crate) use super::values::transitions::normalize_easing;
use super::*;
pub(in crate::engine::css) mod math;
pub(in crate::engine::css) mod numbers;
pub(in crate::engine::css) mod time;

pub(crate) fn parse_length(value: &str) -> Option<Length> {
    let value = value.trim();
    // Bound source storage; recursive complexity is checked by the typed parser.
    if value.len() > 16_384 {
        return None;
    }
    // CSS dimensions are a single token: whitespace cannot separate the number and unit.
    // CSS Values 4 §5.4: https://www.w3.org/TR/css-values-4/#dimensions
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let token = parser.next().ok()?.clone();
    if let Token::Function(ref name) = token {
        return if name.eq_ignore_ascii_case("calc") {
            // Admission before the historical affine parser bounds its recursion
            // too. Tokenization, not raw parentheses, handles CSS comments/escapes.
            let admitted = math::parse(value)?;
            let affine = parse_calc_length(value);
            // A canceled or zero percentage is still a percentage-typed value.
            // Discarding that type accepts invalid <length>-only declarations
            // and incorrectly resolves percentages with an indefinite basis.
            Some(match affine {
                Some(value) if !admitted.has_percentage() || value.has_percentage() => value,
                _ => admitted,
            })
        } else {
            math::parse(value)
        };
    }
    parser.expect_exhausted().ok()?;
    match token {
        Token::Ident(name) if name.eq_ignore_ascii_case("auto") => Some(Length::Auto),
        Token::Number { value: 0.0, .. } => Some(Length::Px(0.0)),
        Token::Percentage { unit_value, .. } if unit_value.is_finite() => {
            Some(Length::Percent(unit_value * 100.0))
        }
        Token::Dimension { value, unit, .. } if value.is_finite() => {
            match unit.to_ascii_lowercase().as_str() {
                "px" => Some(Length::Px(value)),
                unit if absolute_length_scale(unit).is_some() => {
                    Some(Length::Px(value * absolute_length_scale(unit)?))
                }
                "em" => Some(Length::Em(value)),
                "rem" => Some(Length::Rem(value)),
                "vw" | "svw" | "lvw" | "dvw" => Some(Length::Vw(value)),
                "vh" | "svh" | "lvh" | "dvh" => Some(Length::Vh(value)),
                "vmin" | "svmin" | "lvmin" | "dvmin" => Some(Length::Vmin(value)),
                "vmax" | "svmax" | "lvmax" | "dvmax" => Some(Length::Vmax(value)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn absolute_length_scale(unit: &str) -> Option<f32> {
    // CSS Values 4: fixed ratios to the reference pixel, including calc() terms.
    match unit {
        "pt" => Some(96.0 / 72.0),
        "pc" => Some(16.0),
        "in" => Some(96.0),
        "cm" => Some(96.0 / 2.54),
        "mm" => Some(96.0 / 25.4),
        "q" => Some(96.0 / 101.6),
        _ => None,
    }
}

pub(crate) fn parse_opacity(value: &str) -> Option<f32> {
    let opacity = numbers::number(value)
        .or_else(|| numbers::percentage(value).map(|points| points / 100.0))?;
    opacity.is_finite().then(|| opacity.clamp(0.0, 1.0))
}

pub(in crate::engine::css) fn math_number(value: &str) -> Option<f32> {
    math::number(value)
}

pub(in crate::engine::css) fn math_percentage(value: &str) -> Option<f32> {
    math::percentage(value)
}

pub(in crate::engine::css) fn math_radians(value: &str) -> Option<f32> {
    math::radians(value)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct CalcLength {
    px: f32,
    percent: f32,
    em: f32,
    rem: f32,
    vw: f32,
    vh: f32,
    vmin: f32,
    vmax: f32,
}

impl CalcLength {
    fn scaled(self, factor: f32) -> Self {
        Self {
            px: self.px * factor,
            percent: self.percent * factor,
            em: self.em * factor,
            rem: self.rem * factor,
            vw: self.vw * factor,
            vh: self.vh * factor,
            vmin: self.vmin * factor,
            vmax: self.vmax * factor,
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            px: self.px + other.px,
            percent: self.percent + other.percent,
            em: self.em + other.em,
            rem: self.rem + other.rem,
            vw: self.vw + other.vw,
            vh: self.vh + other.vh,
            vmin: self.vmin + other.vmin,
            vmax: self.vmax + other.vmax,
        }
    }

    fn into_length(self) -> Length {
        let non_zero = [
            self.px,
            self.percent,
            self.em,
            self.rem,
            self.vw,
            self.vh,
            self.vmin,
            self.vmax,
        ]
        .into_iter()
        .filter(|value| value.abs() > f32::EPSILON)
        .count();
        if non_zero <= 1 {
            if self.percent.abs() > f32::EPSILON {
                Length::Percent(self.percent)
            } else if self.em.abs() > f32::EPSILON {
                Length::Em(self.em)
            } else if self.rem.abs() > f32::EPSILON {
                Length::Rem(self.rem)
            } else if self.vw.abs() > f32::EPSILON {
                Length::Vw(self.vw)
            } else if self.vh.abs() > f32::EPSILON {
                Length::Vh(self.vh)
            } else if self.vmin.abs() > f32::EPSILON {
                Length::Vmin(self.vmin)
            } else if self.vmax.abs() > f32::EPSILON {
                Length::Vmax(self.vmax)
            } else {
                Length::Px(self.px)
            }
        } else {
            Length::Calc {
                px: self.px,
                percent: self.percent,
                em: self.em,
                rem: self.rem,
                vw: self.vw,
                vh: self.vh,
                vmin: self.vmin,
                vmax: self.vmax,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum CalcValue {
    Number(f32),
    Length(CalcLength),
}

pub(super) fn parse_calc_length(value: &str) -> Option<Length> {
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    parser.expect_function_matching("calc").ok()?;
    let value = parser
        .parse_nested_block(|nested| {
            let value = parse_calc_sum(nested)?;
            nested.expect_exhausted()?;
            Ok::<_, cssparser::ParseError<'_, ()>>(value)
        })
        .ok()?;
    parser.expect_exhausted().ok()?;
    match value {
        CalcValue::Length(length) => Some(length.into_length()),
        CalcValue::Number(_) => None,
    }
}

pub(super) fn parse_calc_sum<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Result<CalcValue, cssparser::ParseError<'i, ()>> {
    let mut value = parse_calc_product(input)?;
    loop {
        let operator = if input.try_parse(|input| input.expect_delim('+')).is_ok() {
            1.0
        } else if input.try_parse(|input| input.expect_delim('-')).is_ok() {
            -1.0
        } else {
            break;
        };
        let right = parse_calc_product(input)?;
        value = match (value, right) {
            (CalcValue::Number(left), CalcValue::Number(right)) => {
                CalcValue::Number(left + operator * right)
            }
            (CalcValue::Length(left), CalcValue::Length(right)) => {
                CalcValue::Length(left.plus(right.scaled(operator)))
            }
            _ => return Err(input.new_custom_error::<(), ()>(())),
        };
    }
    Ok(value)
}

pub(super) fn parse_calc_product<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Result<CalcValue, cssparser::ParseError<'i, ()>> {
    let mut value = parse_calc_value(input)?;
    loop {
        let multiply = if input.try_parse(|input| input.expect_delim('*')).is_ok() {
            true
        } else if input.try_parse(|input| input.expect_delim('/')).is_ok() {
            false
        } else {
            break;
        };
        let right = parse_calc_value(input)?;
        value = match (value, right, multiply) {
            (CalcValue::Number(left), CalcValue::Number(right), true) => {
                CalcValue::Number(left * right)
            }
            (CalcValue::Number(left), CalcValue::Number(right), false)
                if right.abs() > f32::EPSILON =>
            {
                CalcValue::Number(left / right)
            }
            (CalcValue::Length(length), CalcValue::Number(number), true) => {
                CalcValue::Length(length.scaled(number))
            }
            (CalcValue::Number(number), CalcValue::Length(length), true) => {
                CalcValue::Length(length.scaled(number))
            }
            (CalcValue::Length(length), CalcValue::Number(number), false)
                if number.abs() > f32::EPSILON =>
            {
                CalcValue::Length(length.scaled(1.0 / number))
            }
            _ => return Err(input.new_custom_error::<(), ()>(())),
        };
    }
    Ok(value)
}

pub(super) fn parse_calc_value<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Result<CalcValue, cssparser::ParseError<'i, ()>> {
    let token = input.next()?.clone();
    match token {
        Token::Number { value, .. } => Ok(CalcValue::Number(value)),
        Token::Percentage { unit_value, .. } => Ok(CalcValue::Length(CalcLength {
            percent: unit_value * 100.0,
            ..CalcLength::default()
        })),
        Token::Dimension { value, unit, .. } => {
            let mut length = CalcLength::default();
            match unit.to_ascii_lowercase().as_str() {
                "px" => length.px = value,
                unit if absolute_length_scale(unit).is_some() => {
                    length.px = value * absolute_length_scale(unit).unwrap();
                }
                "rem" => length.rem = value,
                "em" => length.em = value,
                "vw" => length.vw = value,
                "vh" => length.vh = value,
                "vmin" => length.vmin = value,
                "vmax" => length.vmax = value,
                _ => return Err(input.new_custom_error::<(), ()>(())),
            }
            Ok(CalcValue::Length(length))
        }
        Token::ParenthesisBlock => input.parse_nested_block(|nested| {
            let value = parse_calc_sum(nested)?;
            nested.expect_exhausted()?;
            Ok(value)
        }),
        Token::Function(name) if name.eq_ignore_ascii_case("calc") => {
            input.parse_nested_block(|nested| {
                let value = parse_calc_sum(nested)?;
                nested.expect_exhausted()?;
                Ok(value)
            })
        }
        _ => Err(input.new_custom_error::<(), ()>(())),
    }
}

#[cfg(test)]
mod tests;
