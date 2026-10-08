//! Typed, bounded calculations using cssparser. The type tag belongs to the
//! parser, not the numeric magnitude: sign(10%) is a deferred number, not px.
use super::*;
use crate::engine::css::values::math::Expression;
use std::sync::Arc;
mod consumers;
#[cfg(test)]
mod function_tests;
mod functions;
mod numeric_source;
pub(in crate::engine::css) use consumers::{
    number, number_expression, number_wide, percentage, percentage_points, radians, seconds,
    time_expression,
};
#[cfg(test)]
mod special_tests;
#[cfg(test)]
mod tests;

const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 256;
type Result<'i, T> = std::result::Result<T, cssparser::ParseError<'i, ()>>;

enum Value {
    Number(Arc<Expression>),
    Length(Arc<Expression>),
    Angle(Arc<Expression>),
    Time(Arc<Expression>),
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Number,
    Length,
    Angle,
    Time,
}

impl Value {
    fn is_number(&self) -> bool {
        matches!(self, Self::Number(_))
    }
    fn kind(&self) -> Kind {
        match self {
            Self::Number(_) => Kind::Number,
            Self::Length(_) => Kind::Length,
            Self::Angle(_) => Kind::Angle,
            Self::Time(_) => Kind::Time,
        }
    }
    fn expression(self) -> Arc<Expression> {
        match self {
            Self::Number(value) | Self::Length(value) | Self::Angle(value) | Self::Time(value) => {
                value
            }
        }
    }
    fn typed(kind: Kind, expression: Arc<Expression>) -> Self {
        match kind {
            Kind::Number => Self::Number(expression),
            Kind::Length => Self::Length(expression),
            Kind::Angle => Self::Angle(expression),
            Kind::Time => Self::Time(expression),
        }
    }
}

#[derive(Default)]
struct Budget {
    nodes: usize,
}

fn parse_value(value: &str) -> Option<Value> {
    if value.len() > 16_384 {
        return None;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let initial = parser.state();
    // Only a function can introduce math constants or unitless calculations.
    if !matches!(parser.next().ok()?, Token::Function(_)) {
        return None;
    }
    parser.reset(&initial);
    let result = Budget::default().value(&mut parser, 0).ok()?;
    parser.expect_exhausted().ok()?;
    Some(result)
}

// Angle consumers also accept a bare CSS dimension. Keep the same tokenizer,
// unit conversion, expression budget and exhaustive-input validation as math.
fn parse_angle_value(value: &str) -> Option<Value> {
    if value.len() > 16_384 {
        return None;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let result = Budget::default().value(&mut parser, 0).ok()?;
    parser.expect_exhausted().ok()?;
    Some(result)
}

pub(super) fn parse(value: &str) -> Option<Length> {
    match parse_value(value)? {
        Value::Length(value) => Some(Length::Math(value)),
        _ => None,
    }
}

impl Budget {
    fn node<'i>(
        &mut self,
        input: &Parser<'i, '_>,
        node: Expression,
    ) -> Result<'i, Arc<Expression>> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(input.new_custom_error(()));
        }
        Ok(Arc::new(node))
    }

    fn sum<'i>(&mut self, input: &mut Parser<'i, '_>, depth: usize) -> Result<'i, Value> {
        let mut value = self.product(input, depth)?;
        loop {
            let checkpoint = input.state();
            if !matches!(input.next_including_whitespace(), Ok(Token::WhiteSpace(_))) {
                input.reset(&checkpoint);
                break;
            }
            let sign = match input.next() {
                Ok(Token::Delim('+')) => 1.0,
                Ok(Token::Delim('-')) => -1.0,
                _ => {
                    input.reset(&checkpoint);
                    break;
                }
            };
            // Comments do not substitute for required CSS + / - whitespace.
            if !matches!(input.next_including_whitespace(), Ok(Token::WhiteSpace(_))) {
                return Err(input.new_custom_error(()));
            }
            let right = self.product(input, depth)?;
            let kind = value.kind();
            if kind != right.kind() {
                return Err(input.new_custom_error(()));
            }
            let right = right.expression();
            let right = if sign < 0.0 {
                self.node(input, Expression::Scale(right, -1.0))?
            } else {
                right
            };
            value = Value::typed(
                kind,
                self.node(input, Expression::Sum(value.expression(), right))?,
            );
        }
        Ok(value)
    }

    fn product<'i>(&mut self, input: &mut Parser<'i, '_>, depth: usize) -> Result<'i, Value> {
        let mut value = self.value(input, depth)?;
        loop {
            let multiply = if input.try_parse(|p| p.expect_delim('*')).is_ok() {
                true
            } else if input.try_parse(|p| p.expect_delim('/')).is_ok() {
                false
            } else {
                break;
            };
            let right = self.value(input, depth)?;
            let kind = if multiply {
                if value.is_number() {
                    right.kind()
                } else if right.is_number() {
                    value.kind()
                } else {
                    return Err(input.new_custom_error(()));
                }
            } else if right.is_number() {
                value.kind()
            } else if value.kind() == right.kind() {
                Kind::Number
            } else {
                return Err(input.new_custom_error(()));
            };
            let right = right.expression();
            let expression = if multiply {
                Expression::Product(value.expression(), right)
            } else {
                Expression::Quotient(value.expression(), right)
            };
            value = Value::typed(kind, self.node(input, expression)?);
        }
        Ok(value)
    }

    fn value<'i>(&mut self, input: &mut Parser<'i, '_>, depth: usize) -> Result<'i, Value> {
        self.nodes += 1;
        if depth > MAX_DEPTH || self.nodes > MAX_NODES {
            return Err(input.new_custom_error(()));
        }
        input.skip_whitespace();
        let start = input.position();
        let token = input.next()?.clone();
        let raw = input.slice_from(start);
        let length = match token {
            Token::Number { .. } => {
                let value = numeric_source::value(raw).ok_or_else(|| input.new_custom_error(()))?;
                return Ok(Value::Number(self.node(input, Expression::Number(value))?));
            }
            Token::Ident(name)
                if ["pi", "e", "infinity", "-infinity", "nan"]
                    .iter()
                    .any(|keyword| name.eq_ignore_ascii_case(keyword)) =>
            {
                let value = match name.to_ascii_lowercase().as_str() {
                    "pi" => std::f64::consts::PI,
                    "e" => std::f64::consts::E,
                    "infinity" => f64::INFINITY,
                    "-infinity" => f64::NEG_INFINITY,
                    _ => f64::NAN,
                };
                return Ok(Value::Number(self.node(input, Expression::Number(value))?));
            }
            Token::Percentage { unit_value, .. } if unit_value.is_finite() => {
                Length::Percent(unit_value * 100.0)
            }
            Token::Dimension { unit, .. } => {
                let value = numeric_source::value(raw).ok_or_else(|| input.new_custom_error(()))?;
                let seconds = match unit.to_ascii_lowercase().as_str() {
                    "s" => Some(value),
                    "ms" => Some(value / 1000.0),
                    _ => None,
                };
                if let Some(value) = seconds {
                    return Ok(Value::Time(self.node(input, Expression::Time(value))?));
                }
                let radians = match unit.to_ascii_lowercase().as_str() {
                    "rad" => Some(value),
                    "deg" => Some(value.to_radians()),
                    "grad" => Some(value * std::f64::consts::PI / 200.0),
                    "turn" => Some(value * std::f64::consts::TAU),
                    _ => None,
                };
                if let Some(value) = radians {
                    return Ok(Value::Angle(self.node(input, Expression::Angle(value))?));
                }
                // Length storage is the existing f32 layout representation.
                // Scalars, angles and times retain f64 throughout evaluation.
                let value = value as f32;
                if !value.is_finite() {
                    return Err(input.new_custom_error(()));
                }
                match unit.to_ascii_lowercase().as_str() {
                    "px" => Length::Px(value),
                    unit if absolute_length_scale(unit).is_some() => {
                        Length::Px(value * absolute_length_scale(unit).unwrap())
                    }
                    "em" => Length::Em(value),
                    "rem" => Length::Rem(value),
                    "vw" | "svw" | "lvw" | "dvw" => Length::Vw(value),
                    "vh" | "svh" | "lvh" | "dvh" => Length::Vh(value),
                    "vmin" | "svmin" | "lvmin" | "dvmin" => Length::Vmin(value),
                    "vmax" | "svmax" | "lvmax" | "dvmax" => Length::Vmax(value),
                    _ => return Err(input.new_custom_error(())),
                }
            }
            Token::ParenthesisBlock => {
                return input.parse_nested_block(|p| {
                    let value = self.sum(p, depth + 1)?;
                    p.expect_exhausted()?;
                    Ok(value)
                });
            }
            Token::Function(name) => {
                return input.parse_nested_block(|p| {
                    let value = self.function(p, &name.to_ascii_lowercase(), depth + 1)?;
                    p.expect_exhausted()?;
                    Ok(value)
                });
            }
            _ => return Err(input.new_custom_error(())),
        };
        Ok(Value::Length(self.node(input, Expression::Value(length))?))
    }

    fn function<'i>(
        &mut self,
        input: &mut Parser<'i, '_>,
        name: &str,
        depth: usize,
    ) -> Result<'i, Value> {
        if depth > MAX_DEPTH {
            return Err(input.new_custom_error(()));
        }
        match name {
            "calc" => self.sum(input, depth),
            "min" | "max" => {
                let values = input.parse_comma_separated(|p| self.sum(p, depth))?;
                let kind = values
                    .first()
                    .ok_or_else(|| input.new_custom_error(()))?
                    .kind();
                if values.iter().any(|v| v.kind() != kind) {
                    return Err(input.new_custom_error(()));
                }
                let values = values.into_iter().map(Value::expression).collect();
                let expression = if name == "min" {
                    Expression::Min(values)
                } else {
                    Expression::Max(values)
                };
                Ok(Value::typed(kind, self.node(input, expression)?))
            }
            "clamp" => {
                let min = self.bound(input, depth)?;
                input.expect_comma()?;
                let value = self.sum(input, depth)?;
                input.expect_comma()?;
                let max = self.bound(input, depth)?;
                let kind = value.kind();
                if min.as_ref().is_some_and(|v| v.kind() != kind)
                    || max.as_ref().is_some_and(|v| v.kind() != kind)
                {
                    return Err(input.new_custom_error(()));
                }
                let expression = Expression::Clamp(
                    min.map(Value::expression),
                    value.expression(),
                    max.map(Value::expression),
                );
                Ok(Value::typed(kind, self.node(input, expression)?))
            }
            _ => self.extended_function(input, name, depth),
        }
    }

    fn bound<'i>(&mut self, input: &mut Parser<'i, '_>, depth: usize) -> Result<'i, Option<Value>> {
        if input.try_parse(|p| p.expect_ident_matching("none")).is_ok() {
            Ok(None)
        } else {
            self.sum(input, depth).map(Some)
        }
    }
}
