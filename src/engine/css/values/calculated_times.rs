//! Deferred computed animation/transition time lists. Retain author trees only
//! until font and viewport context exists; the animation clock consumes seconds.
use super::super::value_parser::{math as parser, time};
use super::*;
type PendingList = Vec<Option<Arc<math::Expression>>>;
type ParsedTimes = (Vec<f32>, PendingList);

pub(crate) fn serialize(seconds: &[f32], pending: &[Option<Arc<math::Expression>>]) -> String {
    seconds
        .iter()
        .enumerate()
        .map(|(index, value)| {
            pending
                .get(index)
                .and_then(Option::as_ref)
                .map_or_else(|| format!("{value}s"), |expression| expression.css_text())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TimeValue {
    Fixed(f32),
    Calculation(Arc<math::Expression>),
}

impl TimeValue {
    pub(crate) fn parse(value: &str, allow_negative: bool) -> Option<Self> {
        time::seconds(value, allow_negative)
            .map(Self::Fixed)
            .or_else(|| parser::time_expression(value).map(Self::Calculation))
    }

    pub(crate) fn append(
        self,
        seconds: &mut Vec<f32>,
        pending: &mut Vec<Option<Arc<math::Expression>>>,
    ) {
        match self {
            Self::Fixed(value) => {
                seconds.push(value);
                pending.push(None);
            }
            Self::Calculation(expression) => {
                seconds.push(0.0);
                pending.push(Some(expression));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CalculatedTimes {
    pub(crate) durations: Vec<Option<Arc<math::Expression>>>,
    pub(crate) delays: Vec<Option<Arc<math::Expression>>>,
}

impl CalculatedTimes {
    pub(crate) fn compact(&mut self) {
        if self.durations.iter().all(Option::is_none) {
            self.durations.clear();
        }
        if self.delays.iter().all(Option::is_none) {
            self.delays.clear();
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.durations.is_empty() || !self.delays.is_empty()
    }

    pub(crate) fn resolve(
        &mut self,
        durations: &mut [f32],
        delays: &mut [f32],
        font: f32,
        root: f32,
        width: f32,
        height: f32,
    ) {
        let evaluate = |value: Arc<math::Expression>| {
            value
                .map_lengths(&|length| {
                    length
                        .clone()
                        .resolve_root_font_units(root)
                        .resolve_viewport_units(width, height)
                })
                .resolve(None, font)
                .unwrap_or(0.0)
        };
        for (slot, value) in durations
            .iter_mut()
            .zip(std::mem::take(&mut self.durations))
        {
            if let Some(value) = value {
                *slot = evaluate(value).max(0.0);
            }
        }
        for (slot, value) in delays.iter_mut().zip(std::mem::take(&mut self.delays)) {
            if let Some(value) = value {
                *slot = evaluate(value);
            }
        }
    }
}

pub(crate) fn list(value: &str, allow_negative: bool) -> Option<ParsedTimes> {
    let mut seconds = Vec::new();
    let mut pending = Vec::new();
    for part in super::super::split_css_top_level(value, ',') {
        if seconds.len() == 64 {
            return None;
        }
        TimeValue::parse(part.trim(), allow_negative)?.append(&mut seconds, &mut pending);
    }
    if seconds.is_empty() {
        return None;
    }
    if pending.iter().all(Option::is_none) {
        pending.clear();
    }
    Some((seconds, pending))
}

impl ComputedStyle {
    pub(in crate::engine::css) fn resolve_calculated_times(
        &mut self,
        width: f32,
        height: f32,
        root: f32,
    ) {
        if self.animation.calculated_times.has_pending() {
            let animation = Arc::make_mut(&mut self.animation);
            animation.calculated_times.resolve(
                &mut animation.durations,
                &mut animation.delays,
                self.font_size,
                root,
                width,
                height,
            );
        }
        self.transition.calculated_times.resolve(
            &mut self.transition.durations,
            &mut self.transition.delays,
            self.font_size,
            root,
            width,
            height,
        );
    }
}
