//! Attribute selector matching, including HTML's default ASCII-insensitive values.

use super::*;

pub(super) fn attribute_matches(selector: &AttributeSelector, node: &NodeRef) -> bool {
    let Some(actual) = node.attr_ref(&selector.name) else {
        return false;
    };
    if matches!(selector.operator, AttributeOperator::Exists) {
        return true;
    }

    let case_insensitive = match selector.case_sensitivity {
        AttributeCaseSensitivity::AsciiInsensitive => true,
        AttributeCaseSensitivity::Sensitive => false,
        AttributeCaseSensitivity::Default => {
            node.namespace_uri() == Some("http://www.w3.org/1999/xhtml")
                && html_case_insensitive_attribute(&selector.name)
        }
    };
    let expected = selector.value.as_str();
    if expected.is_empty() && !matches!(selector.operator, AttributeOperator::Equals) {
        // Selectors 4 defines non-equality substring/token operators with an
        // empty operand as matching nothing; `=` still matches an empty value.
        return false;
    }
    let compare = |left: &str, right: &str| {
        if case_insensitive {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    };
    let normalized_actual;
    let normalized_expected;
    let (actual, expected) = if case_insensitive {
        normalized_actual = actual.to_ascii_lowercase();
        normalized_expected = expected.to_ascii_lowercase();
        (normalized_actual.as_str(), normalized_expected.as_str())
    } else {
        (&*actual, expected)
    };

    match selector.operator {
        AttributeOperator::Exists => true,
        AttributeOperator::Equals => compare(actual, expected),
        AttributeOperator::Includes => actual
            .split_ascii_whitespace()
            .any(|value| compare(value, expected)),
        AttributeOperator::DashMatch => {
            compare(actual, expected)
                || actual
                    .strip_prefix(expected)
                    .is_some_and(|suffix| suffix.starts_with('-'))
        }
        AttributeOperator::Prefix => actual.starts_with(expected),
        AttributeOperator::Suffix => actual.ends_with(expected),
        AttributeOperator::Substring => actual.contains(expected),
    }
}

fn html_case_insensitive_attribute(name: &str) -> bool {
    // HTML Standard §4.16.2, default sensitivity of attribute selector values.
    // The explicit `s` selector modifier overrides this list.
    matches!(
        name,
        "accept"
            | "accept-charset"
            | "align"
            | "alink"
            | "axis"
            | "bgcolor"
            | "charset"
            | "checked"
            | "clear"
            | "codetype"
            | "color"
            | "compact"
            | "declare"
            | "defer"
            | "dir"
            | "direction"
            | "disabled"
            | "enctype"
            | "face"
            | "frame"
            | "hreflang"
            | "http-equiv"
            | "lang"
            | "language"
            | "link"
            | "media"
            | "method"
            | "multiple"
            | "nohref"
            | "noresize"
            | "noshade"
            | "nowrap"
            | "readonly"
            | "rel"
            | "rev"
            | "rules"
            | "scope"
            | "scrolling"
            | "selected"
            | "shape"
            | "target"
            | "text"
            | "type"
            | "valign"
            | "valuetype"
            | "vlink"
    )
}
