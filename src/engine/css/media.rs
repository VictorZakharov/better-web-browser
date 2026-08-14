//! Media-query evaluation.

use super::*;

pub(crate) fn media_matches(prelude: &str, viewport_width: f32) -> bool {
    let queries = prelude
        .trim()
        .strip_prefix("@media")
        .unwrap_or(prelude)
        .trim();
    split_css_top_level(queries, ',').any(|query| media_query_matches(query, viewport_width))
}

pub(super) fn media_query_matches(query: &str, viewport_width: f32) -> bool {
    let mut query = query.trim().to_ascii_lowercase();
    let negated = query.starts_with("not ");
    if negated {
        query = query["not ".len()..].trim().to_string();
    }
    if let Some(rest) = query.strip_prefix("only ") {
        query = rest.trim().to_string();
    }

    let media_type_matches = if query.starts_with("print") || query.starts_with("speech") {
        false
    } else {
        query.starts_with("screen")
            || query.starts_with("all")
            || query.starts_with('(')
            || query.starts_with("and ")
    };

    let mut conditions_match = true;
    let mut cursor = 0;
    let mut found_condition = false;
    while let Some(relative_open) = query[cursor..].find('(') {
        let open = cursor + relative_open;
        let Some(close) = find_matching_parenthesis(&query, open) else {
            conditions_match = false;
            break;
        };
        found_condition = true;
        let condition = query[open + 1..close].trim();
        if !media_condition_matches(condition, viewport_width) {
            conditions_match = false;
            break;
        }
        cursor = close + 1;
    }

    let mut matches =
        media_type_matches && (!query.contains('(') || found_condition) && conditions_match;
    if negated {
        matches = !matches;
    }
    matches
}

pub(super) fn media_condition_matches(condition: &str, viewport_width: f32) -> bool {
    let Some((feature, value)) = condition.split_once(':') else {
        return false;
    };
    let feature = feature.trim();
    let value = value.trim();
    match feature {
        "min-width" => parse_length(value)
            .and_then(|length| length.resolve(viewport_width, 16.0))
            .is_some_and(|minimum| viewport_width >= minimum),
        "max-width" => parse_length(value)
            .and_then(|length| length.resolve(viewport_width, 16.0))
            .is_some_and(|maximum| viewport_width <= maximum),
        "width" => parse_length(value)
            .and_then(|length| length.resolve(viewport_width, 16.0))
            .is_some_and(|expected| (viewport_width - expected).abs() < 0.5),
        "hover" | "any-hover" => value == "hover",
        "pointer" | "any-pointer" => value == "fine",
        // Unknown media features are false per CSS media-query evaluation. In particular,
        // vendor-only fallbacks must never leak into the normal standards style set.
        _ => false,
    }
}
