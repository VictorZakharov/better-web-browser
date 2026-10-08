//! Deferred CSS comparison calculations. The containing-block percentage basis
//! must be known before choosing a branch: min(50%, 200px) is not a fixed length.
//! CSS Values 4 §10.2: https://www.w3.org/TR/css-values-4/#comp-func

use super::Length;
use std::sync::Arc;
mod functions;
pub(crate) use functions::{Function, Rounding};

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Number(f64),
    Angle(f64),
    Time(f64),
    Value(Length),
    Sum(Arc<Self>, Arc<Self>),
    Scale(Arc<Self>, f32),
    Min(Vec<Arc<Self>>),
    Max(Vec<Arc<Self>>),
    Clamp(Option<Arc<Self>>, Arc<Self>, Option<Arc<Self>>),
    Product(Arc<Self>, Arc<Self>),
    Quotient(Arc<Self>, Arc<Self>),
    Function(Function, Vec<Arc<Self>>),
}

impl Expression {
    pub fn into_length(self: Arc<Self>) -> Length {
        // CSS Values 4 §10.10.1: comparisons collapse only once all branches
        // have context; a percentage branch cannot be discarded early.
        if self.is_absolute()
            && let Some(value) = self.resolve(None, 0.0)
        {
            Length::Px(value)
        } else {
            Length::Math(self)
        }
    }

    fn is_absolute(&self) -> bool {
        match self {
            Self::Number(_) | Self::Angle(_) | Self::Time(_) | Self::Value(Length::Px(_)) => true,
            Self::Value(_) => false,
            Self::Sum(left, right) | Self::Product(left, right) | Self::Quotient(left, right) => {
                left.is_absolute() && right.is_absolute()
            }
            Self::Scale(value, _) => value.is_absolute(),
            Self::Min(values) | Self::Max(values) | Self::Function(_, values) => {
                values.iter().all(|v| v.is_absolute())
            }
            Self::Clamp(min, value, max) => {
                min.as_ref().is_none_or(|v| v.is_absolute())
                    && value.is_absolute()
                    && max.as_ref().is_none_or(|v| v.is_absolute())
            }
        }
    }

    pub fn resolve(&self, basis: Option<f32>, font_size: f32) -> Option<f32> {
        Some(
            self.resolve_f64(basis, font_size)?
                .clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32,
        )
    }

    pub(crate) fn resolve_f64(&self, basis: Option<f32>, font_size: f32) -> Option<f64> {
        let value = self.evaluate(basis, font_size)?;
        // CSS Values 4 §10.9.1: censor only at the outer calculation. NaN and
        // signed zero inside a function must still affect its parent operation.
        Some(if value.is_nan() || value == 0.0 {
            0.0
        } else {
            value.clamp(-f64::MAX, f64::MAX)
        })
    }

    fn evaluate(&self, basis: Option<f32>, font_size: f32) -> Option<f64> {
        let value = match self {
            Self::Number(value) | Self::Angle(value) | Self::Time(value) => *value,
            Self::Value(length) => {
                if basis.is_none() && length.has_percentage() {
                    return None;
                }
                f64::from(length.resolve(basis.unwrap_or(0.0), font_size)?)
            }
            Self::Sum(left, right) => {
                left.evaluate(basis, font_size)? + right.evaluate(basis, font_size)?
            }
            Self::Scale(value, scale) => value.evaluate(basis, font_size)? * f64::from(*scale),
            Self::Product(left, right) => {
                left.evaluate(basis, font_size)? * right.evaluate(basis, font_size)?
            }
            Self::Quotient(left, right) => {
                left.evaluate(basis, font_size)? / right.evaluate(basis, font_size)?
            }
            Self::Function(function, values) => function.evaluate(
                values
                    .iter()
                    .map(|value| value.evaluate(basis, font_size))
                    .collect::<Option<Vec<_>>>()?
                    .as_slice(),
            )?,
            Self::Min(values) | Self::Max(values) => {
                let mut resolved = values.iter().map(|value| value.evaluate(basis, font_size));
                let mut result = resolved.next()??;
                for value in resolved {
                    let value = value?;
                    result = if matches!(self, Self::Min(_)) {
                        functions::minimum(result, value)
                    } else {
                        functions::maximum(result, value)
                    };
                }
                result
            }
            Self::Clamp(minimum, value, maximum) => {
                let mut value = value.evaluate(basis, font_size)?;
                if let Some(maximum) = maximum {
                    value = functions::minimum(value, maximum.evaluate(basis, font_size)?);
                }
                // Unlike f32::clamp, CSS defines reversed bounds: minimum wins.
                if let Some(minimum) = minimum {
                    value = functions::maximum(value, minimum.evaluate(basis, font_size)?);
                }
                value
            }
        };
        Some(value)
    }

    pub fn has_percentage(&self) -> bool {
        match self {
            Self::Number(_) | Self::Angle(_) | Self::Time(_) => false,
            Self::Value(value) => value.has_percentage(),
            Self::Sum(left, right) | Self::Product(left, right) | Self::Quotient(left, right) => {
                left.has_percentage() || right.has_percentage()
            }
            Self::Scale(value, _) => value.has_percentage(),
            Self::Min(values) | Self::Max(values) | Self::Function(_, values) => {
                values.iter().any(|v| v.has_percentage())
            }
            Self::Clamp(min, value, max) => {
                min.as_ref().is_some_and(|v| v.has_percentage())
                    || value.has_percentage()
                    || max.as_ref().is_some_and(|v| v.has_percentage())
            }
        }
    }

    pub fn map_lengths(self: &Arc<Self>, map: &impl Fn(&Length) -> Length) -> Arc<Self> {
        // Styles normalize root and viewport units independently. A tree that
        // does not contain those units must not allocate a second copy on every
        // style refresh. Immutable children are shared, never modified in place.
        let changed = std::cell::Cell::new(false);
        let child = |value: &Arc<Self>| {
            let mapped = value.map_lengths(map);
            changed.set(changed.get() || !Arc::ptr_eq(value, &mapped));
            mapped
        };
        let mapped = match self.as_ref() {
            Self::Number(_) | Self::Angle(_) | Self::Time(_) => return Arc::clone(self),
            Self::Value(value) => {
                let mapped = map(value);
                if mapped == *value {
                    return Arc::clone(self);
                }
                return Arc::new(Self::Value(mapped));
            }
            Self::Sum(left, right) => Self::Sum(child(left), child(right)),
            Self::Product(left, right) => Self::Product(child(left), child(right)),
            Self::Quotient(left, right) => Self::Quotient(child(left), child(right)),
            Self::Function(function, values) => {
                Self::Function(*function, values.iter().map(child).collect())
            }
            Self::Scale(value, scale) => Self::Scale(child(value), *scale),
            Self::Min(values) => Self::Min(values.iter().map(child).collect()),
            Self::Max(values) => Self::Max(values.iter().map(child).collect()),
            Self::Clamp(min, value, max) => Self::Clamp(
                min.as_ref().map(child),
                child(value),
                max.as_ref().map(child),
            ),
        };
        if changed.get() {
            Arc::new(mapped)
        } else {
            Arc::clone(self)
        }
    }

    pub fn css_text(&self) -> String {
        let list = |values: &[Arc<Self>]| {
            values
                .iter()
                .map(|v| v.css_text())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            Self::Number(value) => functions::serialize_number(*value),
            Self::Angle(value) | Self::Time(value) => {
                let unit = if matches!(self, Self::Angle(_)) {
                    "rad"
                } else {
                    "s"
                };
                if value.is_finite() && !(*value == 0.0 && value.is_sign_negative()) {
                    format!("{value}{unit}")
                } else {
                    format!("calc({} * 1{unit})", functions::serialize_number(*value))
                }
            }
            Self::Value(value) => super::super::cssom::serialize_length(value.clone()),
            Self::Sum(left, right) => format!("calc({} + {})", left.css_text(), right.css_text()),
            Self::Scale(value, scale) => format!("calc({} * {scale})", value.css_text()),
            Self::Product(left, right) => {
                format!("calc({} * {})", left.css_text(), right.css_text())
            }
            Self::Quotient(left, right) => {
                format!("calc({} / {})", left.css_text(), right.css_text())
            }
            Self::Function(function, values) => function.css_text(&list(values)),
            Self::Min(values) => format!("min({})", list(values)),
            Self::Max(values) => format!("max({})", list(values)),
            Self::Clamp(min, value, max) => format!(
                "clamp({}, {}, {})",
                min.as_ref().map_or_else(|| "none".into(), |v| v.css_text()),
                value.css_text(),
                max.as_ref().map_or_else(|| "none".into(), |v| v.css_text()),
            ),
        }
    }
}
