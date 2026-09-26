//! Selectors Level 4 language matching using RFC 4647 extended filtering.

use super::*;
use unicode_bidi::{BidiClass, bidi_class};

pub(super) fn matches_directions(directions: &[TextDirection], node: &NodeRef) -> bool {
    directions.is_empty()
        || directions
            .iter()
            .all(|direction| *direction == element_direction(node))
}

fn element_direction(node: &NodeRef) -> TextDirection {
    if node.namespace_uri() == Some("http://www.w3.org/1999/xhtml") {
        match node
            .attr_ref("dir")
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("ltr") => return TextDirection::Ltr,
            Some("rtl") => return TextDirection::Rtl,
            Some("auto") => return auto_direction(node).unwrap_or(TextDirection::Ltr),
            _ => {}
        }
    }
    if node.tag_name() == Some("bdi") {
        return auto_direction(node).unwrap_or(TextDirection::Ltr);
    }
    if node.tag_name() == Some("input")
        && node
            .attr_ref("type")
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("tel"))
    {
        return TextDirection::Ltr;
    }
    node.shadow_including_parent()
        .filter(|parent| parent.element().is_some())
        .map_or(TextDirection::Ltr, |parent| element_direction(&parent))
}

fn auto_direction(node: &NodeRef) -> Option<TextDirection> {
    match node.tag_name() {
        Some("input") => return first_strong(&node.input_value()),
        Some("textarea") => return first_strong(&node.textarea_raw()),
        _ => {}
    }
    let mut pending = node
        .children
        .borrow()
        .iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>();
    while let Some(candidate) = pending.pop() {
        if let NodeData::Text(value) = &candidate.data {
            if let Some(direction) = first_strong(&value.borrow()) {
                return Some(direction);
            }
        } else if candidate.element().is_some() {
            if matches!(
                candidate.tag_name(),
                Some("bdi" | "script" | "style" | "textarea")
            ) || candidate.attr_ref("dir").is_some_and(|value| {
                matches!(value.to_ascii_lowercase().as_str(), "ltr" | "rtl" | "auto")
            }) {
                continue;
            }
            pending.extend(candidate.children.borrow().iter().rev().cloned());
        }
    }
    None
}

fn first_strong(text: &str) -> Option<TextDirection> {
    text.chars()
        .find_map(|character| match bidi_class(character) {
            BidiClass::L => Some(TextDirection::Ltr),
            BidiClass::R | BidiClass::AL => Some(TextDirection::Rtl),
            _ => None,
        })
}

pub(super) fn matches_languages(groups: &[Vec<String>], node: &NodeRef) -> bool {
    if groups.is_empty() {
        return true;
    }
    let language = inherited_language(node);
    groups.iter().all(|ranges| {
        ranges
            .iter()
            .any(|range| language_range_matches(range, language.as_deref()))
    })
}

fn inherited_language(node: &NodeRef) -> Option<String> {
    let mut ancestor = Some(node.clone());
    while let Some(current) = ancestor {
        if let Some(language) = current.attr_ref("lang") {
            return Some(language.to_string());
        }
        if let Some(language) = current.attr_ref("xml:lang") {
            return Some(language.to_string());
        }
        ancestor = current.shadow_including_parent();
    }
    None
}

fn language_range_matches(range: &str, language: Option<&str>) -> bool {
    let language = language.unwrap_or("");
    if range.is_empty() {
        return language.is_empty();
    }
    if !valid_language_range(range) || !valid_language_tag(language) {
        return false;
    }
    let range_parts = range.split('-').collect::<Vec<_>>();
    let tag_parts = language.split('-').collect::<Vec<_>>();
    if range_parts[0] != "*" && !range_parts[0].eq_ignore_ascii_case(tag_parts[0]) {
        return false;
    }
    let mut range_index = 1;
    let mut tag_index = 1;
    while range_index < range_parts.len() {
        let wanted = range_parts[range_index];
        if wanted == "*" {
            range_index += 1;
            continue;
        }
        let Some(actual) = tag_parts.get(tag_index) else {
            return false;
        };
        if wanted.eq_ignore_ascii_case(actual) {
            range_index += 1;
            tag_index += 1;
        } else if actual.len() == 1 {
            // RFC 4647: do not skip a singleton extension or private-use subtag.
            return false;
        } else {
            tag_index += 1;
        }
    }
    true
}

fn valid_language_range(range: &str) -> bool {
    let mut parts = range.split('-');
    let Some(first) = parts.next() else {
        return false;
    };
    if first != "*"
        && (first.is_empty()
            || first.len() > 8
            || !first.bytes().all(|byte| byte.is_ascii_alphabetic()))
    {
        return false;
    }
    parts.all(|part| {
        part == "*"
            || (!part.is_empty()
                && part.len() <= 8
                && part.bytes().all(|byte| byte.is_ascii_alphanumeric()))
    })
}

fn valid_language_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let Some(first) = parts.next() else {
        return false;
    };
    !first.is_empty()
        && first.len() <= 8
        && first.bytes().all(|byte| byte.is_ascii_alphabetic())
        && parts.all(|part| {
            !part.is_empty()
                && part.len() <= 8
                && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_ranges_skip_intermediate_subtags_but_not_extensions() {
        assert!(language_range_matches("de-DE", Some("de-Latn-DE-1996")));
        assert!(language_range_matches("*-CH", Some("fr-Latn-CH")));
        assert!(language_range_matches("de-*-DE", Some("de-Latn-DE")));
        assert!(!language_range_matches("de-DE", Some("de-x-DE")));
        assert!(!language_range_matches("fr", Some("de-FR")));
    }

    #[test]
    fn empty_and_wildcard_ranges_differ_for_untagged_content() {
        assert!(language_range_matches("", None));
        assert!(language_range_matches("", Some("")));
        assert!(!language_range_matches("*", None));
        assert!(language_range_matches("*", Some("und")));
        assert!(!language_range_matches("åå", Some("åå")));
    }
}
