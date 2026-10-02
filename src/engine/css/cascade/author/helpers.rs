//! Cascade rollback baselines and tree-origin metadata.
use super::*;

pub(super) fn tree_scope(node: &NodeRef) -> Option<NodeId> {
    let root = Node::tree_root(node);
    root.shadow_host().map(|_| root.id())
}

pub(super) fn rule_scope(scope: RuleScope) -> Option<NodeId> {
    match scope {
        RuleScope::Document => None,
        RuleScope::Shadow(id)
        | RuleScope::Host(id)
        | RuleScope::HostChild(id)
        | RuleScope::Slotted(id) => Some(id),
    }
}

pub(super) fn normal_prefix(
    history: &[(LayerKey, ComputedStyle)],
    origin: &ComputedStyle,
    before: LayerKey,
) -> ComputedStyle {
    history
        .iter()
        .rev()
        .find(|(key, _)| {
            key.context > before.context
                || (key.context == before.context && key.rank < before.rank)
        })
        .map_or_else(|| origin.clone(), |(_, style)| style.clone())
}

pub(super) fn shadow_context_depth(root: &NodeRef) -> u8 {
    let mut depth = 0_u8;
    let mut current = root.clone();
    while let Some(host) = current.shadow_host() {
        depth = depth.saturating_add(1);
        current = Node::tree_root(&host);
    }
    depth
}

pub(super) fn part_context_depth(node: &NodeRef, scope: RuleScope) -> Option<u8> {
    let RuleScope::Shadow(source_root) = scope else {
        return matches!(scope, RuleScope::Document).then_some(0);
    };
    let mut root = Node::tree_root(node);
    loop {
        if root.id() == source_root {
            return Some(shadow_context_depth(&root));
        }
        root = Node::tree_root(&root.shadow_host()?);
    }
}

pub(super) fn might_revert_layer(value: &str) -> bool {
    value.as_bytes().contains(&b'\\')
        || value
            .as_bytes()
            .windows(b"revert-layer".len())
            .any(|part| part.eq_ignore_ascii_case(b"revert-layer"))
}
