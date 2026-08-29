//! Media-query serialization and evaluation against one shared document environment.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MediaEnvironment {
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
    pub(crate) resolution_dppx: f32,
    pub(crate) prefers_dark_color_scheme: bool,
}

impl MediaEnvironment {
    pub(crate) fn new(width: f32, height: f32, dppx: f32, dark: bool) -> Self {
        Self {
            viewport_width: width.max(1.0),
            viewport_height: height.max(1.0),
            resolution_dppx: dppx.max(0.01),
            prefers_dark_color_scheme: dark,
        }
    }

    pub(crate) fn with_viewport(self, width: f32, height: f32) -> Self {
        Self::new(
            width,
            height,
            self.resolution_dppx,
            self.prefers_dark_color_scheme,
        )
    }
}

pub(crate) fn media_matches_for_environment(prelude: &str, environment: MediaEnvironment) -> bool {
    let input = prelude.trim();
    let at_rule = input.starts_with("@media");
    let queries = input.strip_prefix("@media").unwrap_or(input).trim();
    if queries.is_empty() {
        return !at_rule;
    }
    if serialize_media_query_list(queries) == "not all" && queries != "not all" {
        return false;
    }
    split_css_top_level(queries, ',').any(|query| media_query_matches(query, environment))
}

pub(crate) fn media_query_matches(query: &str, environment: MediaEnvironment) -> bool {
    let mut query = query.trim().to_ascii_lowercase();
    let negated = query.starts_with("not ");
    if negated {
        query = query["not ".len()..].trim().to_string();
    }
    if let Some(rest) = query.strip_prefix("only ") {
        query = rest.trim().to_string();
    }
    if query.is_empty() || !balanced_parentheses(&query) {
        return false;
    }

    let first_condition = query.find('(');
    let prefix = first_condition
        .map(|open| query[..open].trim())
        .unwrap_or(query.as_str());
    let media_type = prefix.strip_suffix("and").unwrap_or(prefix).trim();
    let mut matches = match media_type {
        "" if first_condition.is_some() => true,
        "all" | "screen" => true,
        _ => false,
    };
    let mut cursor = first_condition.unwrap_or(query.len());
    let mut previous_close = cursor;
    let mut found_condition = false;
    while cursor < query.len() {
        let Some(relative_open) = query[cursor..].find('(') else {
            break;
        };
        let open = cursor + relative_open;
        let Some(close) = find_matching_parenthesis(&query, open) else {
            return false;
        };
        let connector = query[previous_close..open].trim();
        let condition_matches = media_condition_matches(query[open + 1..close].trim(), environment);
        matches = if found_condition && connector == "or" {
            matches || condition_matches
        } else {
            matches && condition_matches
        };
        found_condition = true;
        previous_close = close + 1;
        cursor = close + 1;
    }
    if first_condition.is_some() && !found_condition {
        matches = false;
    }
    if negated { !matches } else { matches }
}

/// CSSOM View requires invalid query lists to serialize as "not all".
pub(crate) fn serialize_media_query_list(input: &str) -> String {
    let input = input.trim();
    if input.is_empty() {
        return String::new();
    }
    if !valid_media_query_list(input) {
        return "not all".to_string();
    }
    split_css_top_level(input, ',')
        .map(|query| {
            let mut query = query
                .trim()
                .to_ascii_lowercase()
                .split_ascii_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(rest) = query.strip_prefix("only ") {
                query = rest.to_string();
            }
            if let Some(rest) = query.strip_prefix("all and ") {
                query = rest.to_string();
            }
            normalize_colon_spacing(&query)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn valid_media_query_list(input: &str) -> bool {
    balanced_parentheses(input)
        && !input.contains("::")
        && split_css_top_level(input, ',').all(|query| {
            let query = query.trim();
            !query.is_empty()
                && !query.ends_with(" and")
                && !query.ends_with(" or")
                && query != "not"
                && query != "only"
                && !query.contains(['{', '}', ';'])
        })
}

fn balanced_parentheses(input: &str) -> bool {
    let mut depth = 0_u32;
    for character in input.chars() {
        if character == '(' {
            depth += 1;
        } else if character == ')' {
            let Some(next) = depth.checked_sub(1) else {
                return false;
            };
            depth = next;
        }
    }
    depth == 0
}

fn normalize_colon_spacing(input: &str) -> String {
    let mut output = String::with_capacity(input.len() + 4);
    let mut characters = input.chars().peekable();
    while let Some(character) = characters.next() {
        output.push(character);
        if character == ':' {
            while characters
                .peek()
                .is_some_and(|next| next.is_ascii_whitespace())
            {
                characters.next();
            }
            output.push(' ');
        }
    }
    output
}

fn media_condition_matches(condition: &str, environment: MediaEnvironment) -> bool {
    if let Some(result) = range_condition_matches(condition, environment) {
        return result;
    }
    if !condition.contains(':') {
        return matches!(
            condition,
            "width"
                | "height"
                | "orientation"
                | "resolution"
                | "aspect-ratio"
                | "prefers-color-scheme"
                | "hover"
                | "pointer"
        );
    }
    let Some((feature, value)) = condition.split_once(':') else {
        return false;
    };
    let feature = feature.trim();
    let value = value.trim();
    match feature {
        "min-width" | "min-device-width" => {
            length_comparison(value, environment.viewport_width, |a, b| a >= b)
        }
        "max-width" | "max-device-width" => {
            length_comparison(value, environment.viewport_width, |a, b| a <= b)
        }
        "width" | "device-width" => {
            length_comparison(value, environment.viewport_width, approximately_equal)
        }
        "min-height" | "min-device-height" => {
            length_comparison(value, environment.viewport_height, |a, b| a >= b)
        }
        "max-height" | "max-device-height" => {
            length_comparison(value, environment.viewport_height, |a, b| a <= b)
        }
        "height" | "device-height" => {
            length_comparison(value, environment.viewport_height, approximately_equal)
        }
        "orientation" => matches!(
            (
                value,
                environment.viewport_width >= environment.viewport_height
            ),
            ("landscape", true) | ("portrait", false)
        ),
        "min-aspect-ratio" => {
            parse_ratio(value).is_some_and(|ratio| aspect_ratio(environment) >= ratio)
        }
        "max-aspect-ratio" => {
            parse_ratio(value).is_some_and(|ratio| aspect_ratio(environment) <= ratio)
        }
        "aspect-ratio" => parse_ratio(value)
            .is_some_and(|ratio| approximately_equal(aspect_ratio(environment), ratio)),
        "min-resolution" => parse_resolution(value)
            .is_some_and(|resolution| environment.resolution_dppx >= resolution),
        "max-resolution" => parse_resolution(value)
            .is_some_and(|resolution| environment.resolution_dppx <= resolution),
        "resolution" => parse_resolution(value)
            .is_some_and(|resolution| approximately_equal(environment.resolution_dppx, resolution)),
        "hover" | "any-hover" => value == "hover",
        "pointer" | "any-pointer" => value == "fine",
        "prefers-color-scheme" => match value {
            "dark" => environment.prefers_dark_color_scheme,
            "light" => !environment.prefers_dark_color_scheme,
            _ => false,
        },
        "prefers-reduced-motion" | "prefers-contrast" => value == "no-preference",
        "forced-colors" => value == "none",
        "display-mode" => value == "browser",
        "update" => value == "fast",
        "overflow-block" | "overflow-inline" => value == "scroll",
        _ => false,
    }
}

fn length_comparison(value: &str, actual: f32, compare: impl FnOnce(f32, f32) -> bool) -> bool {
    parse_length(value)
        .and_then(|length| length.resolve(actual, 16.0))
        .is_some_and(|expected| compare(actual, expected))
}

fn range_condition_matches(condition: &str, environment: MediaEnvironment) -> Option<bool> {
    let parts = condition.split_ascii_whitespace().collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    let (actual, expected, operator) = if let Some(actual) = feature_value(parts[0], environment) {
        (actual, query_value(parts[0], parts[2], actual)?, parts[1])
    } else {
        let actual = feature_value(parts[2], environment)?;
        (query_value(parts[2], parts[0], actual)?, actual, parts[1])
    };
    Some(match operator {
        "=" => approximately_equal(actual, expected),
        ">" => actual > expected,
        ">=" => actual >= expected,
        "<" => actual < expected,
        "<=" => actual <= expected,
        _ => return None,
    })
}

fn feature_value(feature: &str, environment: MediaEnvironment) -> Option<f32> {
    match feature {
        "width" | "device-width" => Some(environment.viewport_width),
        "height" | "device-height" => Some(environment.viewport_height),
        "resolution" => Some(environment.resolution_dppx),
        "aspect-ratio" => Some(aspect_ratio(environment)),
        _ => None,
    }
}

fn query_value(feature: &str, value: &str, base: f32) -> Option<f32> {
    match feature {
        "resolution" => parse_resolution(value),
        "aspect-ratio" => parse_ratio(value),
        _ => parse_length(value)?.resolve(base, 16.0),
    }
}

fn aspect_ratio(environment: MediaEnvironment) -> f32 {
    environment.viewport_width / environment.viewport_height
}

fn parse_ratio(value: &str) -> Option<f32> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator = numerator.trim().parse::<f32>().ok()?;
    let denominator = denominator.trim().parse::<f32>().ok()?;
    (numerator.is_finite() && denominator > 0.0).then_some(numerator / denominator)
}

fn parse_resolution(value: &str) -> Option<f32> {
    let (number, scale) = value
        .strip_suffix("dppx")
        .map(|number| (number, 1.0))
        .or_else(|| value.strip_suffix("dpi").map(|number| (number, 1.0 / 96.0)))
        .or_else(|| {
            value
                .strip_suffix("dpcm")
                .map(|number| (number, 2.54 / 96.0))
        })?;
    let number = number.trim().parse::<f32>().ok()?;
    (number.is_finite() && number >= 0.0).then_some(number * scale)
}

fn approximately_equal(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.01
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(width: f32, height: f32, dppx: f32, dark: bool) -> MediaEnvironment {
        MediaEnvironment::new(width, height, dppx, dark)
    }

    #[test]
    fn applies_all_media_type_with_a_minimum_width() {
        assert!(media_matches_for_environment(
            "@media all and (min-width: 640px)",
            environment(1088.0, 720.0, 1.0, false)
        ));
        let dom = dom::parse(
            "<style>.logo{display:none}@media all and (min-width:640px){.logo{display:block}}</style><img class=logo>",
        );
        let styles = StyleSet::from_dom(&dom, &[], 1088.0);
        let logo = dom.elements_named("img").next().unwrap();
        assert_eq!(styles.get(&logo).display, Display::Block);
    }

    #[test]
    fn evaluates_viewport_preference_and_resolution_features() {
        let environment = environment(800.0, 600.0, 1.5, true);
        assert!(media_query_matches("(height >= 600px)", environment));
        assert!(media_query_matches("(orientation: landscape)", environment));
        assert!(media_query_matches("(min-resolution: 144dpi)", environment));
        assert!(!media_query_matches(
            "(prefers-color-scheme: light)",
            environment
        ));
    }

    #[test]
    fn stylesheet_media_rules_use_the_same_complete_environment() {
        let dom = dom::parse(
            "<style>.target{display:none}@media (min-height:600px) and (min-resolution:1.5dppx){.target{display:block}}</style><div class=target></div>",
        );
        let styles = StyleSet::from_sources_for_media_environment(
            &dom,
            "",
            &[],
            environment(800.0, 600.0, 1.5, false),
        );
        let target = dom.elements_named("div").next().unwrap();
        assert_eq!(styles.get(&target).display, Display::Block);
    }

    #[test]
    fn serializes_query_lists_and_contains_invalid_queries() {
        assert_eq!(serialize_media_query_list(""), "");
        assert_eq!(
            serialize_media_query_list("all and (max-width:199px), (min-width: 200px)"),
            "(max-width: 199px), (min-width: 200px)"
        );
        assert_eq!(serialize_media_query_list("::"), "not all");
        assert!(media_matches_for_environment(
            "",
            environment(800.0, 600.0, 1.0, false)
        ));
    }
}
