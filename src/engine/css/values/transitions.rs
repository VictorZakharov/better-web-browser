//! Computed CSS Transitions Level 1 settings. The transition engine consumes
//! these values only when an already rendered element changes style.
//! https://drafts.csswg.org/css-transitions-1/#transitions

use super::super::split_css_top_level;
mod linear;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TransitionSettings {
    pub properties: Vec<String>,
    pub durations: Vec<f32>, // seconds
    pub delays: Vec<f32>,    // seconds
    pub easings: Vec<String>,
}

impl Default for TransitionSettings {
    fn default() -> Self {
        Self {
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
            let Some(durations) = time_list(value, false) else {
                return false;
            };
            settings.durations = durations;
        }
        "transition-delay" => {
            let Some(delays) = time_list(value, true) else {
                return false;
            };
            settings.delays = delays;
        }
        "transition-timing-function" => {
            let Some(easings) = easing_list(value) else {
                return false;
            };
            settings.easings = easings;
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

fn time_list(value: &str, allow_negative: bool) -> Option<Vec<f32>> {
    let times = split_css_top_level(value, ',')
        .map(|part| time(part.trim(), allow_negative))
        .collect::<Option<Vec<_>>>()?;
    (!times.is_empty() && times.len() <= 64).then_some(times)
}

pub(super) fn time(value: &str, allow_negative: bool) -> Option<f32> {
    let (number, scale) = if let Some(number) = value.strip_suffix("ms") {
        (number, 0.001)
    } else if let Some(number) = value.strip_suffix('s') {
        (number, 1.0)
    } else {
        return None;
    };
    // Whitespace separates CSS tokens: `1 s` is not a time dimension.
    if number.trim() != number {
        return None;
    }
    let seconds = number.parse::<f32>().ok()? * scale;
    (seconds.is_finite() && (allow_negative || seconds >= 0.0)).then_some(seconds)
}

fn easing_list(value: &str) -> Option<Vec<String>> {
    let easings = split_css_top_level(value, ',')
        .map(|part| easing(part.trim()).map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    (!easings.is_empty() && easings.len() <= 64).then_some(easings)
}

pub(super) fn easing(value: &str) -> Option<&str> {
    if linear::valid(value) {
        return Some(value);
    }
    if matches!(
        value,
        "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end"
    ) {
        return Some(value);
    }
    if let Some(inner) = value
        .strip_prefix("cubic-bezier(")
        .and_then(|part| part.strip_suffix(')'))
    {
        let points = inner
            .split(',')
            .map(|part| part.trim().parse::<f32>().ok())
            .collect::<Option<Vec<_>>>()?;
        if points.len() == 4
            && points.iter().all(|point| point.is_finite())
            && (0.0..=1.0).contains(&points[0])
            && (0.0..=1.0).contains(&points[2])
        {
            return Some(value);
        }
    }
    if let Some(inner) = value
        .strip_prefix("steps(")
        .and_then(|part| part.strip_suffix(')'))
    {
        let parts = inner.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() <= 2
            && let Ok(count) = parts[0].parse::<usize>()
            && count > 0
            && (parts.len() == 1
                || matches!(
                    parts[1],
                    "start" | "end" | "jump-start" | "jump-end" | "jump-both" | "jump-none"
                ))
            && (parts.len() == 1 || parts[1] != "jump-none" || count > 1)
        {
            return Some(value);
        }
    }
    None
}

fn shorthand(value: &str) -> Option<TransitionSettings> {
    let mut settings = TransitionSettings {
        properties: Vec::new(),
        durations: Vec::new(),
        delays: Vec::new(),
        easings: Vec::new(),
    };
    for part in split_css_top_level(value, ',') {
        let tokens = whitespace_components(part);
        if tokens.is_empty() || tokens.len() > 4 || settings.properties.len() >= 64 {
            return None;
        }
        let (mut property, mut duration, mut delay, mut timing) = (None, None, None, None);
        for token in tokens {
            if let Some(seconds) = time(token, true) {
                if duration.is_none() {
                    if seconds < 0.0 {
                        return None;
                    }
                    duration = Some(seconds);
                } else if delay.is_none() {
                    delay = Some(seconds);
                } else {
                    return None;
                }
            } else if easing(token).is_some() && timing.is_none() {
                timing = Some(token);
            } else if property_name(token) && property.is_none() {
                property = Some(token);
            } else {
                return None;
            }
        }
        settings
            .properties
            .push(property.unwrap_or("all").to_string());
        settings.durations.push(duration.unwrap_or(0.0));
        settings.delays.push(delay.unwrap_or(0.0));
        settings.easings.push(timing.unwrap_or("ease").to_string());
    }
    (settings.properties.len() == 1 || !settings.properties.iter().any(|part| part == "none"))
        .then_some(settings)
}

pub(super) fn whitespace_components(value: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = None;
    let mut depth = 0u32;
    for (index, character) in value.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if character.is_ascii_whitespace() && depth == 0 => {
                if let Some(begin) = start.take() {
                    result.push(&value[begin..index]);
                }
            }
            _ => {}
        }
        if !character.is_ascii_whitespace() || depth > 0 {
            start.get_or_insert(index);
        }
    }
    if let Some(begin) = start {
        result.push(&value[begin..]);
    }
    result
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
