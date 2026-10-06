//! Authored CSSOM values use native grammar checks, not computed-value substitution.
//! Preserve variable tokens and relative units; canonicalize animation names and time lists.
use super::super::*;
pub(crate) mod fonts;

pub(crate) const MAX_CSSOM_VALUE_BYTES: usize = 64 * 1024;

pub(crate) fn value(property: &str, input: &str) -> Option<String> {
    if input.len() > MAX_CSSOM_VALUE_BYTES
        || property.len() > 128
        || !supports::cssom_declaration_value(property, input)
    {
        return None;
    }
    let input = input.trim();
    if !variables::contains_valid_variable_reference(input) {
        match property {
            "font-feature-settings" => {
                if let Some(value) = FontFeatures::specified_css_text(input) {
                    return Some(value);
                }
            }
            "font-kerning" => {
                if let Some(value) = FontKerning::parse(input) {
                    return Some(value.css_text().into());
                }
            }
            "font-variant-ligatures" => {
                if let Some(value) = FontLigatures::parse(input) {
                    return Some(value.css_text());
                }
            }
            "font-variant-numeric" => {
                if let Some(value) = FontNumeric::parse(input) {
                    return Some(value.css_text());
                }
            }
            _ => {}
        }
    }
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
        for (name, value) in expand(&declaration.name, &value) {
            if let Some(previous) = result
                .iter_mut()
                .find(|(previous_name, _, _)| previous_name == &name)
            {
                if !previous.2 || declaration.important {
                    *previous = (name, value, declaration.important);
                }
            } else {
                result.push((name, value, declaration.important));
            }
        }
    }
    result
}

// CSSOM shorthand assignments replace their longhands, rather than retaining
// an earlier shorthand in front of newer longhand declarations. Keep authored
// font sizes/units intact; these reset-only font components are always normal.
pub(crate) fn expand(property: &str, value: &str) -> Vec<(String, String)> {
    if property == "font-variant" {
        let values = if css_wide::supports_css_wide_keyword(property, value) {
            Some((value.to_ascii_lowercase(), value.to_ascii_lowercase()))
        } else {
            FontVariants::parse_shorthand(value)
                .map(|(ligatures, numeric)| (ligatures.css_text(), numeric.css_text()))
        };
        if let Some((ligatures, numeric)) = values {
            return vec![
                ("font-variant-ligatures".into(), ligatures),
                ("font-variant-numeric".into(), numeric),
            ];
        }
    }
    if property == "font"
        && let Some(expanded) = fonts::expand(value)
    {
        return expanded;
    }
    vec![(property.to_owned(), value.to_owned())]
}

pub(crate) fn font_variant_value(ligatures: &str, numeric: &str) -> String {
    if ligatures == numeric && css_wide::supports_css_wide_keyword("font-variant", ligatures) {
        return ligatures.to_owned();
    }
    match (FontLigatures::parse(ligatures), FontNumeric::parse(numeric)) {
        (Some(ligatures), Some(numeric)) => FontVariants::css_text(ligatures, numeric),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
