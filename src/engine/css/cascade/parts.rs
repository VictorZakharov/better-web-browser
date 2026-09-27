//! CSS Shadow §5 part exposure across tree boundaries.
//! https://drafts.csswg.org/css-shadow-1/#exposing-a-shadow-element

use super::*;
use crate::engine::css::selector_parser::parse_css_identifier;

pub(super) fn part_rule_applies(rule: &Rule, node: &NodeRef, tree_root: &NodeRef) -> bool {
    let Some(part) = rule.part.as_ref() else {
        return false;
    };
    let Some(attribute) = node.attr("part") else {
        return false;
    };
    let mut names = attribute
        .split_ascii_whitespace()
        .map(str::to_string)
        .collect::<HashSet<_>>();
    if names.is_empty() {
        return false;
    }
    let mut root = tree_root.clone();
    while let Some(host) = root.shadow_host() {
        let host_root = Node::tree_root(&host);
        if origin_scope_matches(rule.scope, &host_root)
            && part.names.iter().all(|name| names.contains(name))
            && selector_matches(&part.origin, &host)
        {
            return true;
        }
        let Some(exportparts) = host.attr("exportparts") else {
            break;
        };
        names = forwarded_names(&names, &exportparts);
        if names.is_empty() {
            break;
        }
        root = host_root;
    }
    false
}

fn origin_scope_matches(scope: RuleScope, root: &NodeRef) -> bool {
    match scope {
        RuleScope::Document => !matches!(root.data, NodeData::ShadowRoot(_)),
        RuleScope::Shadow(id) => root.id() == id,
        _ => false,
    }
}

fn forwarded_names(names: &HashSet<String>, attribute: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    // §5.6: comma-separated mappings; malformed entries are skipped independently.
    // Pseudo-element forwarding is deliberately outside this element-part slice.
    for entry in attribute.split(',') {
        let entry = entry.trim_matches(css_space);
        let (inner, outer) = if let Some((inner, outer)) = entry.split_once(':') {
            (inner.trim_matches(css_space), outer.trim_matches(css_space))
        } else {
            (entry, entry)
        };
        if let (Some(inner), Some(outer)) = (css_ident(inner), css_ident(outer))
            && names.contains(&inner)
        {
            result.insert(outer);
        }
    }
    result
}

fn css_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

fn css_ident(input: &str) -> Option<String> {
    parse_css_identifier(input, 0)
        .and_then(|(decoded, end)| (end == input.len()).then_some(decoded))
}
