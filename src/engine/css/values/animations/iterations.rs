//! Iteration numbers may depend on font lengths; the keyword remains distinct
//! from a calculated infinity, which clamps to the numeric range.
use super::super::scalars::NumberValue;
use super::*;
type PendingIterations = Vec<Option<NumberValue>>;
type ParsedIterations = (Vec<f64>, PendingIterations);

#[derive(Clone, Debug, PartialEq)]
pub(super) enum IterationValue {
    Infinite,
    Number(NumberValue),
}

impl IterationValue {
    pub(super) fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("infinite") {
            Some(Self::Infinite)
        } else {
            NumberValue::nonnegative(value).map(Self::Number)
        }
    }

    pub(super) fn append(self, values: &mut Vec<f64>, pending: &mut Vec<Option<NumberValue>>) {
        match self {
            Self::Infinite => {
                values.push(f64::INFINITY);
                pending.push(None);
            }
            Self::Number(NumberValue::Fixed(value)) => {
                values.push(f64::from(value));
                pending.push(None);
            }
            Self::Number(value) => {
                values.push(1.0);
                pending.push(Some(value));
            }
        }
    }
}

pub(super) fn list(value: &str) -> Option<ParsedIterations> {
    let mut values = Vec::new();
    let mut pending = Vec::new();
    for part in split_css_top_level(value, ',') {
        if values.len() == 64 {
            return None;
        }
        IterationValue::parse(part.trim())?.append(&mut values, &mut pending);
    }
    compact(&mut pending);
    (!values.is_empty()).then_some((values, pending))
}

pub(super) fn compact(pending: &mut Vec<Option<NumberValue>>) {
    if pending.iter().all(Option::is_none) {
        pending.clear();
    }
}

pub(super) fn serialize(values: &[f64], pending: &[Option<NumberValue>]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            if let Some(Some(expression)) = pending.get(index) {
                expression.css_text()
            } else if value.is_infinite() {
                "infinite".into()
            } else {
                value.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl super::super::ComputedStyle {
    pub(in crate::engine::css) fn resolve_iterations(
        &mut self,
        width: f32,
        height: f32,
        root: f32,
    ) {
        if self.animation.calculated_iterations.is_empty() {
            return;
        }
        let animation = Arc::make_mut(&mut self.animation);
        for (slot, expression) in animation
            .iterations
            .iter_mut()
            .zip(std::mem::take(&mut animation.calculated_iterations))
        {
            if let Some(expression) = expression {
                *slot = f64::from(
                    expression
                        .resolve(self.font_size, root, width, height)
                        .max(0.0),
                );
            }
        }
    }
}
