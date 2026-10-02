//! Authored CSSOM values use native grammar checks, not computed-value substitution.
//! Preserve variable tokens and relative units; canonicalize animation names and time lists.
use super::super::*;

pub(crate) const MAX_CSSOM_VALUE_BYTES: usize = 64 * 1024;

pub(crate) fn value(property: &str, input: &str) -> Option<String> {
    if input.len() > MAX_CSSOM_VALUE_BYTES
        || property.len() > 128
        || !supports::cssom_declaration_value(property, input)
    {
        return None;
    }
    let input = input.trim();
    if values::animations::supported_property(property)
        && !variables::contains_valid_variable_reference(input)
    {
        let mut settings = values::animations::AnimationSettings::default();
        if values::animations::apply(&mut settings, property, input)
            && let Some(value) = settings.property_value(property)
        {
            return Some(value);
        }
        if css_wide::supports_css_wide_keyword(property, input) {
            return Some(input.to_ascii_lowercase());
        }
    }
    Some(input.to_owned())
}

pub(crate) fn list(source: &str, keyframe: bool) -> Vec<(String, String, bool)> {
    let (source, _) =
        crate::limits::bounded_utf8_prefix(source, crate::limits::MAX_CSS_SOURCE_BYTES);
    let mut result = Vec::<(String, String, bool)>::new();
    for declaration in parse_declarations(source) {
        if keyframe
            && (declaration.important
                || values::animations::supported_property(&declaration.name)
                    && declaration.name != "animation-timing-function")
        {
            continue;
        }
        let Some(value) = value(&declaration.name, &declaration.value) else {
            continue;
        };
        if let Some(previous) = result
            .iter_mut()
            .find(|(name, _, _)| name == &declaration.name)
        {
            if !previous.2 || declaration.important {
                *previous = (declaration.name, value, declaration.important);
            }
        } else {
            result.push((declaration.name, value, declaration.important));
        }
    }
    result
}

#[cfg(test)]
mod tests;
