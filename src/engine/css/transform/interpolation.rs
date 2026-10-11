//! CSS Transforms 1 §11–13: identity padding, primitive interpolation, then matrix fallback.
//! Percentage translations remain deferred; they must not become pixels at parse time.
use super::*;
use crate::engine::css::values::math::Expression;
mod matrix;

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
    let mut result = Vec::new();
    for index in 0..first.0.len().max(last.0.len()) {
        let a = first
            .0
            .get(index)
            .cloned()
            .unwrap_or_else(|| identity(&last.0[index]));
        let b = last
            .0
            .get(index)
            .cloned()
            .unwrap_or_else(|| identity(&first.0[index]));
        if let Some(value) = primitive(&a, &b, progress) {
            result.push(value);
        } else {
            let a = &first.0[index.min(first.0.len())..];
            let b = &last.0[index.min(last.0.len())..];
            if a.iter()
                .chain(b)
                .all(|v| matches!(v, Operation::Translate(_)))
            {
                let axis = |list: &[Operation], x| {
                    let values = list
                        .iter()
                        .map(|v| {
                            let Operation::Translate(v) = v else {
                                unreachable!()
                            };
                            Arc::new(Expression::Value(if x { v.x.clone() } else { v.y.clone() }))
                        })
                        .collect::<Vec<_>>();
                    sum(&values)
                };
                let blend = |x| {
                    Length::Math(Arc::new(Expression::Sum(
                        Arc::new(Expression::Scale(axis(a, x), (1.0 - progress) as f32)),
                        Arc::new(Expression::Scale(axis(b, x), progress as f32)),
                    )))
                };
                result.push(Operation::Translate(TranslateOperation {
                    x: blend(true),
                    y: blend(false),
                }));
            } else {
                result.extend(matrix::interpolate(a, b, progress)?);
            }
            break;
        }
    }
    let result = serialize_transform(&TransformList(result));
    // Composed authored expressions can exceed the ordinary admission budget.
    // Keep its fail-closed contract; a rejected interpolation stays discrete.
    parse_transform(&result)?;
    Some(result)
}

fn primitive(a: &Operation, b: &Operation, p: f64) -> Option<Operation> {
    let number = |a: f32, b: f32| {
        let v = f64::from(a) * (1.0 - p) + f64::from(b) * p;
        (v.is_finite() && v.abs() <= f64::from(f32::MAX)).then_some(v as f32)
    };
    match (a, b) {
        (Operation::Translate(a), Operation::Translate(b)) => {
            Some(Operation::Translate(TranslateOperation {
                x: blend_length(&a.x, &b.x, p),
                y: blend_length(&a.y, &b.y, p),
            }))
        }
        (Operation::Scale(a), Operation::Scale(b)) => {
            Some(Operation::Scale([number(a[0], b[0])?, number(a[1], b[1])?]))
        }
        (Operation::Rotate(a), Operation::Rotate(b)) => Some(Operation::Rotate(number(*a, *b)?)),
        (Operation::Skew(a), Operation::Skew(b)) => {
            Some(Operation::Skew([number(a[0], b[0])?, number(a[1], b[1])?]))
        }
        _ => None,
    }
}

fn identity(operation: &Operation) -> Operation {
    match operation {
        Operation::Translate(_) => Operation::Translate(TranslateOperation {
            x: Length::Px(0.0),
            y: Length::Px(0.0),
        }),
        Operation::Scale(_) => Operation::Scale([1.0, 1.0]),
        Operation::Rotate(_) => Operation::Rotate(0.0),
        Operation::Skew(_) => Operation::Skew([0.0, 0.0]),
        Operation::Matrix(_) => Operation::Matrix(crate::engine::affine::Affine2D::IDENTITY),
    }
}

fn blend_length(a: &Length, b: &Length, p: f64) -> Length {
    values::length_interpolation::blend(a, b, p)
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
