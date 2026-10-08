//! Typed consumer admission is separate from expression grammar construction.
use super::*;
pub(in crate::engine::css) fn number(value: &str) -> Option<f32> {
    number_wide(value).map(|value| value.clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32)
}

pub(in crate::engine::css) fn number_wide(value: &str) -> Option<f64> {
    match parse_value(value)? {
        Value::Number(value) => context_free_wide(&value),
        _ => None,
    }
}

pub(in crate::engine::css) fn number_expression(value: &str) -> Option<Arc<Expression>> {
    let Value::Number(expression) = parse_value(value)? else {
        return None;
    };
    // A scalar consumer has no percentage basis. Font and viewport lengths
    // can resolve at computed-value time, but percentages cannot be guessed.
    (!expression.has_percentage()).then_some(expression)
}

pub(in crate::engine::css) fn percentage(value: &str) -> Option<f32> {
    percentage_points(value).map(|value| value / 100.0)
}

pub(in crate::engine::css) fn percentage_points(value: &str) -> Option<f32> {
    let Value::Length(expression) = parse_value(value)? else {
        return None;
    };
    // Raw percentages (opacity) are not length-percentage mixtures. Scalar
    // multiplication is valid; adding a number or a px leaf is not.
    let invalid = std::cell::Cell::new(false);
    expression.map_lengths(&|length| {
        if !matches!(length, Length::Percent(_)) {
            invalid.set(true);
        }
        length.clone()
    });
    if invalid.get() {
        None
    } else {
        // Evaluate in percentage points directly. A fraction-first f32
        // round-trip would serialize 20% as 20.000000298023224% in easing.
        expression.resolve(Some(100.0), 0.0)
    }
}

pub(in crate::engine::css) fn seconds(value: &str) -> Option<f32> {
    match parse_value(value)? {
        Value::Time(expression) => context_free(&expression),
        _ => None,
    }
}

pub(in crate::engine::css) fn time_expression(value: &str) -> Option<Arc<Expression>> {
    let Value::Time(expression) = parse_value(value)? else {
        return None;
    };
    (!expression.has_percentage()).then_some(expression)
}

pub(in crate::engine::css) fn radians(value: &str) -> Option<f32> {
    match parse_value(value)? {
        Value::Angle(expression) => context_free(&expression),
        _ => None,
    }
}

fn context_free(expression: &Arc<Expression>) -> Option<f32> {
    context_free_wide(expression)
        .map(|value| value.clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32)
}

fn context_free_wide(expression: &Arc<Expression>) -> Option<f64> {
    let relative = std::cell::Cell::new(false);
    expression.map_lengths(&|length| {
        if !matches!(length, Length::Px(_)) {
            relative.set(true);
        }
        length.clone()
    });
    if relative.get() {
        None
    } else {
        expression.resolve_f64(None, 0.0)
    }
}
