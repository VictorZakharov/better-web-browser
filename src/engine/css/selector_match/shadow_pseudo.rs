//! CSS Shadow selectors whose truth depends on slot distribution.

use super::*;
use std::collections::HashSet;

/// CSS Shadow §3.2.5: fallback children do not satisfy `:has-slotted`.
/// https://drafts.csswg.org/css-shadow-1/#has-slotted-pseudo
pub(super) fn matches_has_slotted(slot: &NodeRef) -> bool {
    if slot.tag_name() != Some("slot")
        || !matches!(Node::tree_root(slot).data, NodeData::ShadowRoot(_))
    {
        return false;
    }
    let mut pending = Node::assigned_nodes(slot, false);
    if pending.is_empty() {
        return false;
    }
    let mut seen = HashSet::new();
    while let Some(node) = pending.pop() {
        if !seen.insert(node.id()) {
            continue;
        }
        if node.tag_name() == Some("slot")
            && matches!(Node::tree_root(&node).data, NodeData::ShadowRoot(_))
        {
            let mut nested = Node::assigned_nodes(&node, false);
            if nested.is_empty() {
                nested = node
                    .children
                    .borrow()
                    .iter()
                    .filter(|child| matches!(child.data, NodeData::Element(_) | NodeData::Text(_)))
                    .cloned()
                    .collect();
            }
            pending.extend(nested);
        } else if matches!(node.data, NodeData::Element(_) | NodeData::Text(_)) {
            return true;
        }
    }
    false
}
