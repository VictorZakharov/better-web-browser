//! Length, calc(), and color parsing.

use super::*;

pub(crate) fn parse_length(value: &str) -> Option<Length> {
    let value = value.trim().trim_end_matches("!important").trim();
    if value
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("calc("))
        && value.ends_with(')')
    {
        return parse_calc_length(value);
    }
    if value == "auto" {
        return Some(Length::Auto);
    }
    if value == "0" {
        return Some(Length::Px(0.0));
    }
    for (suffix, constructor) in [
        ("px", Length::Px as fn(f32) -> Length),
        ("pt", |value| Length::Px(value * 96.0 / 72.0)),
        ("em", Length::Em),
        ("rem", |value| Length::Px(value * 16.0)),
        ("vw", Length::Vw),
        ("vh", Length::Vh),
        ("vmin", Length::Vmin),
        ("vmax", Length::Vmax),
        ("%", Length::Percent),
    ] {
        if let Some(number) = value.strip_suffix(suffix)
            && let Ok(number) = number.trim().parse::<f32>()
        {
            return Some(constructor(number));
        }
    }
    None
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct CalcLength {
    px: f32,
    percent: f32,
    em: f32,
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
                "pt" => length.px = value * 96.0 / 72.0,
                "rem" => length.px = value * 16.0,
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

pub(super) fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim().trim_end_matches("!important").trim();
    if value.eq_ignore_ascii_case("transparent") {
        return Some(Color::TRANSPARENT);
    }
    if let Some(hex) = value.strip_prefix('#')
        && let Ok((red, green, blue, alpha)) = parse_hash_color(hex.as_bytes())
    {
        return Some(Color {
            red,
            green,
            blue,
            alpha: (alpha * 255.0).round() as u8,
        });
    }
    if let Ok((red, green, blue)) = parse_named_color(value) {
        return Some(Color::rgb(red, green, blue));
    }
    let lower = value.to_ascii_lowercase();
    let function = lower
        .strip_prefix("rgb(")
        .and_then(|value| value.strip_suffix(')'))
        .map(|value| (value, false))
        .or_else(|| {
            lower
                .strip_prefix("rgba(")
                .and_then(|value| value.strip_suffix(')'))
                .map(|value| (value, true))
        })?;
    let components = function
        .0
        .split([',', ' '])
        .filter(|part| !part.is_empty() && *part != "/")
        .collect::<Vec<_>>();
    if components.len() < 3 {
        return None;
    }
    let channel = |component: &str| -> Option<u8> {
        if let Some(percent) = component.strip_suffix('%') {
            Some(
                (percent.parse::<f32>().ok()? * 2.55)
                    .round()
                    .clamp(0.0, 255.0) as u8,
            )
        } else {
            Some(component.parse::<f32>().ok()?.round().clamp(0.0, 255.0) as u8)
        }
    };
    let alpha = if function.1 && components.len() >= 4 {
        if let Some(percent) = components[3].strip_suffix('%') {
            (percent.parse::<f32>().ok()? * 2.55)
                .round()
                .clamp(0.0, 255.0) as u8
        } else {
            (components[3].parse::<f32>().ok()? * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        }
    } else {
        255
    };
    Some(Color {
        red: channel(components[0])?,
        green: channel(components[1])?,
        blue: channel(components[2])?,
        alpha,
    })
}

pub(super) fn consume_identifier(bytes: &[u8], start: usize) -> usize {
    let mut cursor = start;
    while cursor < bytes.len()
        && (bytes[cursor].is_ascii_alphanumeric()
            || matches!(bytes[cursor], b'-' | b'_')
            || bytes[cursor] >= 0x80)
    {
        cursor += 1;
    }
    cursor
}
