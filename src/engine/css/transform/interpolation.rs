//! Translations compose additively, so mismatched lists can use the matrix
//! fallback without introducing unsupported rotation/scale painting. Endpoint
//! lengths stay deferred until the real box/font/percentage basis is known.
use super::*;
use crate::engine::css::values::math::Expression;

pub(crate) fn interpolate(from: &str, to: &str, progress: f64) -> Option<String> {
    if !progress.is_finite() || progress.abs() > f64::from(f32::MAX) {
        return None;
    }
    let first = parse_transform(from)?;
    let last = parse_transform(to)?;
    if first.is_none() && last.is_none() {
        return Some("none".into());
    }
    if progress == 0.0 {
        return Some(serialize_transform(&first));
    }
    if progress == 1.0 {
        return Some(serialize_transform(&last));
    }
    let axis = |list: &TransformList, x| {
        let values = list
            .0
            .iter()
            .map(|operation| {
                Arc::new(Expression::Value(if x {
                    operation.x.clone()
                } else {
                    operation.y.clone()
                }))
            })
            .collect::<Vec<_>>();
        sum(&values)
    };
    let blend = |x| {
        Length::Math(Arc::new(Expression::Sum(
            Arc::new(Expression::Scale(axis(&first, x), (1.0 - progress) as f32)),
            Arc::new(Expression::Scale(axis(&last, x), progress as f32)),
        )))
    };
    let result = serialize_transform(&TransformList(vec![TranslateOperation {
        x: blend(true),
        y: blend(false),
    }]));
    // Composed authored expressions can exceed the ordinary admission budget.
    // Keep its fail-closed contract; a rejected interpolation stays discrete.
    parse_transform(&result)?;
    Some(result)
}

fn sum(values: &[Arc<Expression>]) -> Arc<Expression> {
    match values {
        [] => Arc::new(Expression::Value(Length::Px(0.0))),
        [value] => Arc::clone(value),
        _ => {
            let middle = values.len() / 2;
            Arc::new(Expression::Sum(
                sum(&values[..middle]),
                sum(&values[middle..]),
            ))
        }
    }
}

#[cfg(test)]
mod tests;
