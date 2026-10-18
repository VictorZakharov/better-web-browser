//! `class` attribute changes carried to the cascade, which bounds their restyle scope.
use super::super::dom::{NodeId, NodeRef};

/// Class tokens that were added or removed on one element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassChange {
    pub node: NodeId,
    pub tokens: Vec<String>,
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
    changes: &mut Vec<ClassChange>,
    node: &NodeRef,
    tokens: Vec<String>,
) {
    merge_class_change(
        changes,
        ClassChange {
            node: node.id(),
            tokens,
        },
    );
}

pub(super) fn merge_class_change(changes: &mut Vec<ClassChange>, change: ClassChange) {
    if let Some(existing) = changes
        .iter_mut()
        .find(|existing| existing.node == change.node)
    {
        existing.tokens.extend(change.tokens);
        existing.tokens.sort_unstable();
        existing.tokens.dedup();
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
