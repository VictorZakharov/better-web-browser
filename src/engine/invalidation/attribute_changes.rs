//! Attribute changes carried to the cascade, which bounds their restyle scope from the rules
//! that read them.
use super::super::dom::{NodeId, NodeRef};

/// Class tokens that were added or removed on one element, and the names of other attributes
/// whose values changed on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeChange {
    pub node: NodeId,
    pub tokens: Vec<String>,
    /// ASCII-lowercase names of attributes for which `selector_only_attribute` holds.
    pub attributes: Vec<String>,
}

/// Whether a change of attribute `name` can affect rendering only through selectors that name
/// it (and `attr()` on the element itself). HTML gives `data-*`, `aria-*` and `role` no
/// presentational hints, user-agent pseudo-class state or effect on other elements, so the
/// cascade can bound their changes by the rules that read them. Every other attribute keeps the
/// parent-subtree scope: `lang` and `dir` are inherited by descendants' `:lang()`/`:dir()`, table
/// attributes style the cells, and form attributes feed validation and other pseudo-classes.
/// https://html.spec.whatwg.org/multipage/dom.html#embedding-custom-non-visible-data-with-the-data-*-attributes
/// https://w3c.github.io/html-aam/
pub(crate) fn selector_only_attribute(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("data-") || name.starts_with("aria-") || name == "role"
}

/// The symmetric difference of two class lists, or `None` when they hold the same tokens.
pub(crate) fn changed_class_tokens(old: &str, new: &str) -> Option<Vec<String>> {
    let old_tokens = old.split_ascii_whitespace().collect::<Vec<_>>();
    let new_tokens = new.split_ascii_whitespace().collect::<Vec<_>>();
    let mut tokens = old_tokens
        .iter()
        .filter(|token| !new_tokens.contains(token))
        .chain(
            new_tokens
                .iter()
                .filter(|token| !old_tokens.contains(token)),
        )
        .map(|token| token.to_string())
        .collect::<Vec<_>>();
    tokens.sort_unstable();
    tokens.dedup();
    (!tokens.is_empty()).then_some(tokens)
}

/// Adds `tokens` for `node`, coalescing repeated changes to the same element.
pub(crate) fn record_class_change(
    changes: &mut Vec<AttributeChange>,
    node: &NodeRef,
    tokens: Vec<String>,
) {
    merge_attribute_change(
        changes,
        AttributeChange {
            node: node.id(),
            tokens,
            attributes: Vec::new(),
        },
    );
}

/// Adds a change of attribute `name` for `node`, coalescing repeated changes to the same element.
pub(crate) fn record_attribute_change(
    changes: &mut Vec<AttributeChange>,
    node: &NodeRef,
    name: &str,
) {
    merge_attribute_change(
        changes,
        AttributeChange {
            node: node.id(),
            tokens: Vec::new(),
            attributes: vec![name.to_ascii_lowercase()],
        },
    );
}

pub(super) fn merge_attribute_change(changes: &mut Vec<AttributeChange>, change: AttributeChange) {
    if let Some(existing) = changes
        .iter_mut()
        .find(|existing| existing.node == change.node)
    {
        existing.tokens.extend(change.tokens);
        existing.tokens.sort_unstable();
        existing.tokens.dedup();
        existing.attributes.extend(change.attributes);
        existing.attributes.sort_unstable();
        existing.attributes.dedup();
    } else {
        changes.push(change);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_differences_ignore_order_and_duplicates() {
        assert_eq!(changed_class_tokens("a b", "b  a a"), None);
        assert_eq!(
            changed_class_tokens("a b c", "c d"),
            Some(vec!["a".into(), "b".into(), "d".into()])
        );
        assert_eq!(changed_class_tokens("", "x"), Some(vec!["x".into()]));
    }
}
