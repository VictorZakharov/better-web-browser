//! Shadow-tree selectors targeting the host or slot-assigned elements.
//! https://drafts.csswg.org/css-shadow-1/#shadow-dom-and-selectors

use super::{NodeId, RuleScope, find_matching_parenthesis};

pub(super) struct ScopedSelector<'a> {
    pub(super) source: &'a str,
    pub(super) scope: RuleScope,
    pub(super) host_condition: Option<&'a str>,
    pub(super) host_context: bool,
    pub(super) slotted_origin: Option<&'a str>,
    pub(super) slotted_host_child: bool,
}

pub(super) fn scoped_selector(selector: &str, scope: RuleScope) -> Option<ScopedSelector<'_>> {
    let RuleScope::Shadow(root) = scope else {
        return Some(ScopedSelector {
            source: selector,
            scope,
            host_condition: None,
            host_context: false,
            slotted_origin: None,
            slotted_host_child: false,
        });
    };
    if let Some(start) = top_level_slotted_start(selector) {
        let open = start + "::slotted".len();
        if selector.as_bytes().get(open) != Some(&b'(') {
            return None;
        }
        let close = find_matching_parenthesis(selector, open)?;
        if close + 1 != selector.len() {
            // Tree-abiding pseudo-elements after ::slotted() are a separate slice.
            return None;
        }
        let argument = selector[open + 1..close].trim();
        let origin = selector[..start].trim();
        if argument.is_empty() {
            return None;
        }
        let origin = scoped_selector(if origin.is_empty() { "*" } else { origin }, scope)?;
        if matches!(origin.scope, RuleScope::Host(_) | RuleScope::Slotted(_)) {
            return None;
        }
        return Some(ScopedSelector {
            source: argument,
            scope: RuleScope::Slotted(root),
            host_condition: origin.host_condition,
            host_context: origin.host_context,
            slotted_origin: Some(origin.source),
            slotted_host_child: matches!(origin.scope, RuleScope::HostChild(_)),
        });
    }
    if selector
        .get(..":host-context".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(":host-context"))
    {
        let open = ":host-context".len();
        if selector.as_bytes().get(open) != Some(&b'(') {
            return None;
        }
        let close = find_matching_parenthesis(selector, open)?;
        let condition = selector[open + 1..close].trim();
        if condition.is_empty() {
            return None;
        }
        let remainder = &selector[close + 1..];
        if remainder.is_empty() {
            return Some(ScopedSelector {
                source: "*",
                scope: RuleScope::Host(root),
                host_condition: Some(condition),
                host_context: true,
                slotted_origin: None,
                slotted_host_child: false,
            });
        }
        let mut parsed = host_selector(condition, remainder, root)?;
        parsed.host_context = true;
        return Some(parsed);
    }
    if selector
        .get(..":host".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(":host"))
    {
        let remainder = &selector[":host".len()..];
        if remainder.starts_with('(') {
            let close = find_matching_parenthesis(selector, ":host".len())?;
            let condition = selector[":host".len() + 1..close].trim();
            if condition.is_empty() {
                return None;
            }
            return host_selector(condition, &selector[close + 1..], root);
        }
        return host_selector("*", remainder, root);
    }
    // Unsupported shadow-specific selector forms must not become ordinary
    // document selectors and style nodes outside the shadow tree.
    if has_unhandled_shadow_pseudo(selector) {
        return None;
    }
    Some(ScopedSelector {
        source: selector,
        scope,
        host_condition: None,
        host_context: false,
        slotted_origin: None,
        slotted_host_child: false,
    })
}

/// Only a top-level pseudo-element can originate a slotted selector; quoted
/// attribute values and functional-selector arguments are ordinary text here.
fn top_level_slotted_start(selector: &str) -> Option<usize> {
    let bytes = selector.as_bytes();
    let mut quote = None;
    let mut brackets = 0usize;
    let mut parentheses = 0usize;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
            continue;
        }
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b'(' if brackets == 0 => parentheses += 1,
                b')' if brackets == 0 => parentheses = parentheses.saturating_sub(1),
                b':' if brackets == 0
                    && parentheses == 0
                    && selector[cursor..]
                        .get(.."::slotted".len())
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("::slotted")) =>
                {
                    return Some(cursor);
                }
                _ => {}
            }
        }
        cursor += 1;
    }
    None
}

fn has_unhandled_shadow_pseudo(selector: &str) -> bool {
    let bytes = selector.as_bytes();
    let mut quote = None;
    let mut attribute_depth = 0usize;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
            continue;
        }
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'[' => attribute_depth += 1,
                b']' => attribute_depth = attribute_depth.saturating_sub(1),
                b':' if attribute_depth == 0 => {
                    let tail = &selector[cursor..];
                    if tail
                        .get(..":host".len())
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(":host"))
                        || tail
                            .get(.."::slotted".len())
                            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("::slotted"))
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        cursor += 1;
    }
    false
}

fn host_selector<'a>(
    condition: &'a str,
    remainder: &'a str,
    root: NodeId,
) -> Option<ScopedSelector<'a>> {
    if remainder.is_empty() {
        return Some(ScopedSelector {
            source: condition,
            scope: RuleScope::Host(root),
            host_condition: None,
            host_context: false,
            slotted_origin: None,
            slotted_host_child: false,
        });
    }
    let separated = remainder.len() != remainder.trim_start().len();
    let remainder = remainder.trim_start();
    if let Some(target) = remainder.strip_prefix('>') {
        let target = target.trim_start();
        return (!target.is_empty()).then_some(ScopedSelector {
            source: target,
            scope: RuleScope::HostChild(root),
            host_condition: Some(condition),
            host_context: false,
            slotted_origin: None,
            slotted_host_child: false,
        });
    }
    if !separated || remainder.is_empty() || remainder.starts_with(['+', '~']) {
        return None;
    }
    Some(ScopedSelector {
        source: remainder,
        scope: RuleScope::Shadow(root),
        host_condition: Some(condition),
        host_context: false,
        slotted_origin: None,
        slotted_host_child: false,
    })
}
