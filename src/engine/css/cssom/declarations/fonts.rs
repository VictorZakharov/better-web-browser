//! CSSOM font shorthand expansion/serialization preserves specified values.
//! Reset-only components must also have initial values for shorthand emission.
use super::*;

pub(crate) const LONGHANDS: [&str; 9] = [
    "font-style",
    "font-weight",
    "font-size",
    "line-height",
    "font-family",
    "font-feature-settings",
    "font-kerning",
    "font-variant-ligatures",
    "font-variant-numeric",
];

pub(super) fn expand(value: &str) -> Option<Vec<(String, String)>> {
    if variables::contains_valid_variable_reference(value) {
        return None;
    }
    let values = if css_wide::supports_css_wide_keyword("font", value) {
        vec![value.to_ascii_lowercase(); LONGHANDS.len()]
    } else {
        let parsed = super::super::super::shorthands::font::parse(value)?;
        vec![
            parsed.style,
            parsed.weight.to_string(),
            parsed.size,
            parsed.line_height,
            parsed.family,
            "normal".into(),
            "auto".into(),
            "normal".into(),
            "normal".into(),
        ]
    };
    Some(
        LONGHANDS
            .into_iter()
            .map(str::to_owned)
            .zip(values)
            .collect(),
    )
}

pub(crate) fn serialize(values: &[String]) -> String {
    if values.len() != LONGHANDS.len() || values.iter().any(String::is_empty) {
        return String::new();
    }
    if values.iter().all(|value| value == &values[0])
        && css_wide::supports_css_wide_keyword("font", &values[0])
    {
        return values[0].clone();
    }
    if values
        .iter()
        .any(|value| css_wide::supports_css_wide_keyword("font", value))
        || values[5] != "normal"
        || values[6] != "auto"
        || values[7] != "normal"
        || values[8] != "normal"
    {
        return String::new();
    }
    let mut parts = Vec::new();
    if values[0] != "normal" {
        parts.push(values[0].clone());
    }
    if !matches!(values[1].as_str(), "normal" | "400") {
        parts.push(values[1].clone());
    }
    parts.push(values[2].clone());
    if values[3] != "normal" {
        parts.push(format!("/ {}", values[3]));
    }
    parts.push(values[4].clone());
    let value = parts.join(" ");
    if super::super::super::shorthands::font::parse(&value).is_some() {
        value
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests;
