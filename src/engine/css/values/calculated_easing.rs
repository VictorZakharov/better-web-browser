//! Linear easing outputs can be number calculations with font/viewport leaves.
//! Retain those outputs until computed style; input percentages stay percentage
//! typed. This does not extend context-dependent cubic-bezier or steps grammar.
use super::super::{syntax, value_parser::numbers};
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EasingValue {
    Fixed(String),
    Linear(Vec<LinearStop>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LinearStop {
    output: scalars::NumberValue,
    positions: Vec<f32>,
}

impl EasingValue {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        let value = normalized.as_str();
        if let Some(fixed) = transitions::easing(value) {
            return Some(Self::Fixed(fixed));
        }
        if value.len() > 16_384 {
            return None;
        }
        let (name, body) = syntax::function(value)?;
        if !name.eq_ignore_ascii_case("linear") {
            return None;
        }
        let tokens = syntax::borrowed_components(body)?;
        let stops = tokens.split(|token| *token == ",").collect::<Vec<_>>();
        if !(2..=64).contains(&stops.len()) {
            return None;
        }
        let mut result = Vec::with_capacity(stops.len());
        for stop in stops {
            if !(1..=3).contains(&stop.len()) {
                return None;
            }
            let mut output = None;
            let mut positions = Vec::new();
            for (index, token) in stop.iter().enumerate() {
                if let Some(number) = scalars::NumberValue::number(token) {
                    if output.replace(number).is_some() || index != 0 && index != stop.len() - 1 {
                        return None;
                    }
                } else {
                    positions.push(numbers::percentage(token)?);
                }
            }
            if positions.len() > 2 {
                return None;
            }
            result.push(LinearStop {
                output: output?,
                positions,
            });
        }
        Some(Self::Linear(result))
    }

    fn text(&self, output: impl Fn(&scalars::NumberValue) -> String) -> String {
        match self {
            Self::Fixed(value) => value.clone(),
            Self::Linear(stops) => format!(
                "linear({})",
                stops
                    .iter()
                    .map(|stop| {
                        let positions = stop
                            .positions
                            .iter()
                            .map(|position| format!(" {position}%"))
                            .collect::<String>();
                        format!("{}{positions}", output(&stop.output))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    pub(crate) fn css_text(&self) -> String {
        self.text(scalars::NumberValue::css_text)
    }

    pub(crate) fn resolve(&self, font: f32, root: f32, width: f32, height: f32) -> String {
        self.text(|value| value.resolve(font, root, width, height).to_string())
    }

    pub(crate) fn append(self, values: &mut Vec<String>, pending: &mut Vec<Option<Self>>) {
        match self {
            Self::Fixed(value) => {
                values.push(value);
                pending.push(None);
            }
            calculated => {
                values.push("ease".into());
                pending.push(Some(calculated));
            }
        }
    }
}

pub(crate) fn compact(values: &mut Vec<Option<EasingValue>>) {
    if values.iter().all(Option::is_none) {
        values.clear();
    }
}

pub(crate) fn list(value: &str) -> Option<(Vec<String>, Vec<Option<EasingValue>>)> {
    let mut values = Vec::new();
    let mut pending = Vec::new();
    for part in super::super::split_css_top_level(value, ',') {
        if values.len() == 64 {
            return None;
        }
        EasingValue::parse(part.trim())?.append(&mut values, &mut pending);
    }
    if values.is_empty() {
        return None;
    }
    compact(&mut pending);
    Some((values, pending))
}

pub(crate) fn serialize(values: &[String], pending: &[Option<EasingValue>]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            pending
                .get(index)
                .and_then(Option::as_ref)
                .map_or_else(|| value.clone(), EasingValue::css_text)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn resolve(
    values: &mut [String],
    pending: &mut Vec<Option<EasingValue>>,
    font: f32,
    root: f32,
    width: f32,
    height: f32,
) {
    for (slot, value) in values.iter_mut().zip(std::mem::take(pending)) {
        if let Some(value) = value {
            *slot = value.resolve(font, root, width, height);
        }
    }
}

impl ComputedStyle {
    pub(in crate::engine::css) fn resolve_calculated_easings(
        &mut self,
        width: f32,
        height: f32,
        root: f32,
    ) {
        if !self.animation.calculated_easings.is_empty() {
            let animation = Arc::make_mut(&mut self.animation);
            resolve(
                &mut animation.easings,
                &mut animation.calculated_easings,
                self.font_size,
                root,
                width,
                height,
            );
        }
        resolve(
            &mut self.transition.easings,
            &mut self.transition.calculated_easings,
            self.font_size,
            root,
            width,
            height,
        );
    }
}
