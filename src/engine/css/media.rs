//! Media-query evaluation.

use super::*;

pub(crate) fn media_matches(prelude: &str, viewport_width: f32) -> bool {
    media_matches_with_color_scheme(prelude, viewport_width, false)
}

pub(crate) fn media_matches_with_color_scheme(
    prelude: &str,
    viewport_width: f32,
    prefers_dark_color_scheme: bool,
) -> bool {
    let queries = prelude
        .trim()
        .strip_prefix("@media")
        .unwrap_or(prelude)
        .trim();
    split_css_top_level(queries, ',').any(|query| {
        media_query_matches_with_color_scheme(query, viewport_width, prefers_dark_color_scheme)
    })
}

pub(crate) fn media_query_matches_with_color_scheme(
    query: &str,
    viewport_width: f32,
    prefers_dark_color_scheme: bool,
) -> bool {
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
        if !media_condition_matches_with_color_scheme(
            condition,
            viewport_width,
            prefers_dark_color_scheme,
        ) {
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

fn media_condition_matches_with_color_scheme(
    condition: &str,
    viewport_width: f32,
    prefers_dark_color_scheme: bool,
) -> bool {
    if condition.trim() == "prefers-color-scheme" {
        return true;
    }
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
        "prefers-color-scheme" => match value {
            "dark" => prefers_dark_color_scheme,
            "light" => !prefers_dark_color_scheme,
            _ => false,
        },
        // Unknown media features are false per CSS media-query evaluation. In particular,
        // vendor-only fallbacks must never leak into the normal standards style set.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_all_media_type_with_a_minimum_width() {
        assert!(media_matches("@media all and (min-width: 640px)", 1088.0));
        let dom = dom::parse(
            "<style>.logo{display:none}@media all and (min-width:640px){.logo{display:block}}</style><img class=logo>",
        );
        let styles = StyleSet::from_dom(&dom, &[], 1088.0);
        let logo = dom.elements_named("img").next().unwrap();
        assert_eq!(styles.get(&logo).display, Display::Block);
    }

    #[test]
    fn evaluates_the_preferred_color_scheme_from_the_media_environment() {
        assert!(media_query_matches_with_color_scheme(
            "(prefers-color-scheme: dark)",
            1088.0,
            true
        ));
        assert!(!media_query_matches_with_color_scheme(
            "(prefers-color-scheme: light)",
            1088.0,
            true
        ));
        assert!(media_query_matches_with_color_scheme(
            "(prefers-color-scheme: light)",
            1088.0,
            false
        ));
    }
}
