//! Computed CSS Transitions Level 1 settings. The transition engine consumes
//! these values only when an already rendered element changes style.
//! https://drafts.csswg.org/css-transitions-1/#transitions

use super::super::split_css_top_level;
use super::calculated_easing::{self, EasingValue};
use super::calculated_times::{self, CalculatedTimes, TimeValue};
mod computed_times;
mod easing;
#[cfg(test)]
mod linear;
#[cfg(test)]
mod math_tests;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TransitionSettings {
    pub(crate) calculated_times: CalculatedTimes,
    pub(crate) calculated_easings: Vec<Option<EasingValue>>,
    pub properties: Vec<String>,
    pub durations: Vec<f32>, // seconds
    pub delays: Vec<f32>,    // seconds
    pub easings: Vec<String>,
}

impl Default for TransitionSettings {
    fn default() -> Self {
        Self {
            calculated_times: CalculatedTimes::default(),
            calculated_easings: Vec::new(),
            properties: vec!["all".into()],
            durations: vec![0.0],
            delays: vec![0.0],
            easings: vec!["ease".into()],
        }
    }
}

pub(crate) fn supported_property(name: &str) -> bool {
    matches!(
        name,
        "transition"
            | "transition-property"
            | "transition-duration"
            | "transition-delay"
            | "transition-timing-function"
    )
}

pub(crate) fn supports(name: &str, value: &str) -> bool {
    let mut settings = TransitionSettings::default();
    apply(&mut settings, name, value)
}

/// Return false for an invalid declaration without changing an earlier cascade winner.
pub(crate) fn apply(settings: &mut TransitionSettings, name: &str, value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    let value = normalized.as_str();
    match name {
        "transition-property" => {
            let Some(properties) = property_list(value) else {
                return false;
            };
            settings.properties = properties;
        }
        "transition-duration" => {
            let Some((durations, pending)) = calculated_times::list(value, false) else {
                return false;
            };
            settings.durations = durations;
            settings.calculated_times.durations = pending;
        }
        "transition-delay" => {
            let Some((delays, pending)) = calculated_times::list(value, true) else {
                return false;
            };
            settings.delays = delays;
            settings.calculated_times.delays = pending;
        }
        "transition-timing-function" => {
            let Some((easings, pending)) = calculated_easing::list(value) else {
                return false;
            };
            settings.easings = easings;
            settings.calculated_easings = pending;
        }
        "transition" => {
            let Some(next) = shorthand(value) else {
                return false;
            };
            *settings = next;
        }
        _ => return false,
    }
    true
}

fn property_list(value: &str) -> Option<Vec<String>> {
    let properties = split_css_top_level(value, ',')
        .map(|part| part.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    if properties.is_empty()
        || properties.len() > 64
        || properties.iter().any(|part| !property_name(part))
        || (properties.len() > 1 && properties.iter().any(|part| part == "none"))
    {
        return None;
    }
    Some(properties)
}

fn property_name(value: &str) -> bool {
    !value.is_empty()
        && !matches!(
            value,
            "initial" | "inherit" | "unset" | "revert" | "revert-layer"
        )
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || byte == b'-'
                || byte == b'_'
                || (index > 0 && byte.is_ascii_digit())
        })
}

pub(super) fn easing(value: &str) -> Option<String> {
    let normalized = normalize_easing(value)?;
    // Preserve existing authored formatting for elementary functions. Math
    // arguments and comments need a resolved string for the shared sampler.
    if value.contains("calc(")
        || value.contains("min(")
        || value.contains("max(")
        || value.matches('(').count() > 1
        || value.contains("/*")
        || value.contains('\\')
    {
        Some(normalized)
    } else {
        Some(value.to_owned())
    }
}

pub(crate) fn normalize_easing(value: &str) -> Option<String> {
    easing::normalize(value)
}

fn shorthand(value: &str) -> Option<TransitionSettings> {
    let mut settings = TransitionSettings {
        calculated_times: CalculatedTimes::default(),
        calculated_easings: Vec::new(),
        properties: Vec::new(),
        durations: Vec::new(),
        delays: Vec::new(),
        easings: Vec::new(),
    };
    for part in split_css_top_level(value, ',') {
        let tokens = super::super::syntax::borrowed_components(part)?;
        if tokens.is_empty() || tokens.len() > 4 || settings.properties.len() >= 64 {
            return None;
        }
        let (mut property, mut duration, mut delay, mut timing) = (None, None, None, None);
        for token in tokens {
            if let Some(seconds) = TimeValue::parse(token, true) {
                if duration.is_none() {
                    duration = Some(TimeValue::parse(token, false)?);
                } else if delay.is_none() {
                    delay = Some(seconds);
                } else {
                    return None;
                }
            } else if EasingValue::parse(token).is_some() && timing.is_none() {
                timing = EasingValue::parse(token);
            } else if property_name(token) && property.is_none() {
                property = Some(token);
            } else {
                return None;
            }
        }
        settings
            .properties
            .push(property.unwrap_or("all").to_string());
        duration.unwrap_or(TimeValue::Fixed(0.0)).append(
            &mut settings.durations,
            &mut settings.calculated_times.durations,
        );
        delay
            .unwrap_or(TimeValue::Fixed(0.0))
            .append(&mut settings.delays, &mut settings.calculated_times.delays);
        timing
            .unwrap_or_else(|| EasingValue::Fixed("ease".into()))
            .append(&mut settings.easings, &mut settings.calculated_easings);
    }
    settings.calculated_times.compact();
    calculated_easing::compact(&mut settings.calculated_easings);
    (settings.properties.len() == 1 || !settings.properties.iter().any(|part| part == "none"))
        .then_some(settings)
}

pub(crate) fn serialize_times(values: &[f32]) -> String {
    values
        .iter()
        .map(|value| format!("{value}s"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_assigns_repeated_indexed_components() {
        let mut settings = TransitionSettings::default();
        assert!(apply(
            &mut settings,
            "transition",
            "opacity 250ms ease-in 50ms, transform 1s linear -100ms"
        ));
        assert_eq!(settings.properties, ["opacity", "transform"]);
        assert_eq!(settings.durations, [0.25, 1.0]);
        assert_eq!(settings.delays, [0.05, -0.1]);
        assert_eq!(settings.easings, ["ease-in", "linear"]);
    }

    #[test]
    fn invalid_declarations_do_not_replace_valid_settings() {
        let mut settings = TransitionSettings::default();
        assert!(apply(&mut settings, "transition-duration", "100ms"));
        let valid = settings.clone();
        assert!(!apply(&mut settings, "transition-duration", "-2s"));
        assert!(!apply(&mut settings, "transition-duration", "1 s"));
        assert!(!apply(&mut settings, "transition-delay", "-.5 ms"));
        assert!(!apply(
            &mut settings,
            "transition-property",
            "opacity, none"
        ));
        assert!(!apply(&mut settings, "transition", "opacity 2s 3s 4s"));
        assert_eq!(settings, valid);
    }
}
