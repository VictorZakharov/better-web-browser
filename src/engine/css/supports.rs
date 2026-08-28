//! CSS Conditional Rules feature-query evaluation.

use super::*;

/// Evaluates the declaration-query subset of `@supports` against capabilities this engine
/// actually implements. General-enclosed and selector queries stay false until supported.
/// https://www.w3.org/TR/css-conditional-3/#at-supports
pub(super) fn supports_matches(prelude: &str) -> bool {
    let condition = prelude
        .trim()
        .strip_prefix("@supports")
        .unwrap_or(prelude)
        .trim();
    evaluate_condition(condition)
}

fn evaluate_condition(condition: &str) -> bool {
    let condition = condition.trim();
    if let Some(rest) = strip_keyword(condition, "not") {
        return !evaluate_condition(rest);
    }
    if let Some(parts) = split_boolean(condition, "or") {
        return parts.into_iter().any(evaluate_condition);
    }
    if let Some(parts) = split_boolean(condition, "and") {
        return parts.into_iter().all(evaluate_condition);
    }
    let Some(inner) = strip_outer_parentheses(condition) else {
        return false;
    };
    if let Some((property, value)) = split_css_once(inner, ':') {
        supports_declaration(property.trim(), value.trim())
    } else {
        evaluate_condition(inner)
    }
}

fn supports_declaration(property: &str, value: &str) -> bool {
    if property.is_empty() || value.is_empty() || value.ends_with("!important") {
        return false;
    }
    if property.starts_with("--") {
        return true;
    }
    let property = property.to_ascii_lowercase();
    let value = value.to_ascii_lowercase();
    if value == "inherit" {
        return matches!(
            property.as_str(),
            "background"
                | "background-color"
                | "background-image"
                | "mask"
                | "-webkit-mask"
                | "mask-image"
                | "-webkit-mask-image"
                | "background-repeat"
                | "background-position"
                | "background-size"
                | "box-sizing"
                | "color"
                | "font-family"
                | "font-size"
                | "letter-spacing"
                | "word-spacing"
                | "line-height"
                | "max-width"
                | "width"
        );
    }
    if matches!(
        value.as_str(),
        "initial" | "unset" | "revert" | "revert-layer"
    ) {
        return false;
    }
    match property.as_str() {
        "display" => matches!(
            value.as_str(),
            "none"
                | "contents"
                | "block"
                | "inline"
                | "inline-block"
                | "inline-flex"
                | "-webkit-inline-flex"
                | "flex"
                | "-webkit-flex"
                | "-webkit-box"
                | "grid"
                | "-ms-grid"
                | "table"
                | "table-row"
                | "table-cell"
        ),
        "position" => matches!(value.as_str(), "static" | "relative" | "absolute" | "fixed"),
        "float" => matches!(value.as_str(), "none" | "left" | "right"),
        "box-sizing" | "-webkit-box-sizing" => {
            matches!(value.as_str(), "content-box" | "border-box")
        }
        "visibility" => matches!(value.as_str(), "visible" | "hidden" | "collapse"),
        "overflow" | "overflow-x" | "overflow-y" => {
            matches!(value.as_str(), "visible" | "hidden" | "clip")
        }
        "color" | "background-color" | "border-color" => parse_color(&value).is_some(),
        "width"
        | "height"
        | "min-width"
        | "min-height"
        | "max-width"
        | "max-height"
        | "top"
        | "right"
        | "bottom"
        | "left"
        | "margin-top"
        | "margin-right"
        | "margin-bottom"
        | "margin-left"
        | "padding-top"
        | "padding-right"
        | "padding-bottom"
        | "padding-left"
        | "border-top-width"
        | "border-right-width"
        | "border-bottom-width"
        | "border-left-width"
        | "column-gap"
        | "grid-column-gap"
        | "row-gap"
        | "grid-row-gap"
        | "flex-basis"
        | "-webkit-flex-basis"
        | "-moz-flex-basis" => parse_length(&value).is_some(),
        "opacity" => value.parse::<f32>().is_ok_and(f32::is_finite),
        "background-image" | "mask" | "-webkit-mask" | "mask-image" | "-webkit-mask-image" => {
            value == "none" || value.starts_with("url(")
        }
        "background-position" => parse_background_position(&value).is_some(),
        "background-position-x" => parse_background_axis(&value, true).is_some(),
        "background-position-y" => parse_background_axis(&value, false).is_some(),
        "background-size" => parse_background_size(&value).is_some(),
        "background-repeat" => {
            let repeats = value.split_ascii_whitespace().collect::<Vec<_>>();
            (1..=2).contains(&repeats.len())
                && repeats
                    .iter()
                    .all(|repeat| matches!(*repeat, "repeat" | "no-repeat"))
        }
        "font-size" => parse_font_size(&value, 16.0).is_some(),
        "font-weight" => {
            matches!(value.as_str(), "normal" | "bold" | "bolder" | "lighter")
                || value.parse::<u16>().is_ok()
        }
        "font-style" => matches!(value.as_str(), "normal" | "italic" | "oblique"),
        "font-family" => !first_font_family(&value).is_empty(),
        "letter-spacing" | "word-spacing" => parse_text_spacing(&value, 16.0).is_some(),
        "line-height" => parse_line_height(&value, 16.0).is_some(),
        "text-align" => matches!(
            value.as_str(),
            "left" | "start" | "center" | "right" | "end"
        ),
        "white-space" => matches!(value.as_str(), "normal" | "nowrap" | "pre" | "pre-wrap"),
        "text-decoration" | "text-decoration-line" => {
            matches!(value.as_str(), "none" | "underline")
        }
        "list-style" | "list-style-type" => matches!(value.as_str(), "none" | "disc"),
        "margin" | "padding" | "border-width" => edge_lengths_supported(&value),
        "border-radius" => parse_length(&value).is_some(),
        "justify-content" | "-webkit-justify-content" | "-webkit-box-pack" => matches!(
            value.as_str(),
            "start"
                | "flex-start"
                | "left"
                | "end"
                | "flex-end"
                | "right"
                | "center"
                | "space-between"
                | "space-around"
                | "space-evenly"
                | "justify"
        ),
        "align-items" | "-webkit-align-items" | "-webkit-box-align" => matches!(
            value.as_str(),
            "stretch" | "start" | "flex-start" | "end" | "flex-end" | "center"
        ),
        "justify-self" => matches!(
            value.as_str(),
            "stretch" | "start" | "flex-start" | "left" | "end" | "flex-end" | "right" | "center"
        ),
        "flex-direction" | "-webkit-flex-direction" | "-moz-flex-direction" => {
            matches!(value.as_str(), "row" | "column")
        }
        "flex-wrap" | "-webkit-flex-wrap" | "-moz-flex-wrap" => {
            matches!(value.as_str(), "nowrap" | "wrap")
        }
        "flex-flow" | "-webkit-flex-flow" | "-moz-flex-flow" => flex_flow_supported(&value),
        "flex-grow"
        | "-webkit-flex-grow"
        | "-moz-flex-grow"
        | "-webkit-box-flex"
        | "flex-shrink"
        | "-webkit-flex-shrink"
        | "-moz-flex-shrink" => value
            .parse::<f32>()
            .is_ok_and(|number| number.is_finite() && number >= 0.0),
        _ => false,
    }
}

fn flex_flow_supported(value: &str) -> bool {
    let mut direction = false;
    let mut wrap = false;
    let mut count = 0;
    for token in value.split_ascii_whitespace() {
        count += 1;
        match token {
            "row" | "column" if !direction => direction = true,
            "nowrap" | "wrap" if !wrap => wrap = true,
            _ => return false,
        }
    }
    (1..=2).contains(&count)
}

fn edge_lengths_supported(value: &str) -> bool {
    let lengths = value.split_ascii_whitespace().collect::<Vec<_>>();
    (1..=4).contains(&lengths.len()) && lengths.iter().all(|length| parse_length(length).is_some())
}

fn strip_keyword<'a>(condition: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = condition.get(keyword.len()..)?;
    condition[..keyword.len()]
        .eq_ignore_ascii_case(keyword)
        .then(|| rest.trim_start())
        .filter(|rest| !rest.is_empty())
}

fn split_boolean<'a>(condition: &'a str, keyword: &str) -> Option<Vec<&'a str>> {
    let bytes = condition.as_bytes();
    let mut depth = 0_i32;
    let mut start = 0_usize;
    let mut parts = Vec::new();
    let mut cursor = 0_usize;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'(' => depth += 1,
            b')' => depth = (depth - 1).max(0),
            _ if depth == 0 && keyword_at(condition, cursor, keyword) => {
                parts.push(condition[start..cursor].trim());
                cursor += keyword.len();
                start = cursor;
                continue;
            }
            _ => {}
        }
        cursor += 1;
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(condition[start..].trim());
    parts.iter().all(|part| !part.is_empty()).then_some(parts)
}

fn keyword_at(input: &str, index: usize, keyword: &str) -> bool {
    let Some(candidate) = input.get(index..index + keyword.len()) else {
        return false;
    };
    candidate.eq_ignore_ascii_case(keyword)
        && input[..index]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
        && input[index + keyword.len()..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
}

fn strip_outer_parentheses(input: &str) -> Option<&str> {
    input
        .starts_with('(')
        .then(|| find_matching_parenthesis(input, 0))
        .flatten()
        .filter(|close| *close + 1 == input.len())
        .map(|close| input[1..close].trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_queries_are_conservative_about_unimplemented_values() {
        assert!(supports_matches("@supports (display: grid)"));
        assert!(!supports_matches("@supports (position: sticky)"));
        assert!(!supports_matches(
            "@supports (grid-template-columns: subgrid)"
        ));
        assert!(supports_matches("@supports (justify-self: center)"));
    }
}
