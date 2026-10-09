//! A conservative raster-work proof, not a CSSOM or DOM visibility API.
use crate::engine::css::{Display, StyleSet};
use crate::engine::dom::{Node, NodeRef};

/// Only computed display:none ancestry suppresses an entire drawing subtree.
/// Unknown styles, visibility:hidden and opacity:0 cannot establish this proof.
pub(super) fn suppressed(node: &NodeRef, styles: &StyleSet) -> bool {
    let top_layer =
        std::iter::successors(Some(node.clone()), |node| node.shadow_including_parent())
            .any(|node| node.is_fullscreen() || node.is_popover_open());
    let parent = |node: &NodeRef| {
        if top_layer {
            // A hidden assigned slot is not a top-layer ancestor. A real
            // display:none ancestor still suppresses a top-layer subtree.
            // https://drafts.csswg.org/css-position-4/#top-layer-styling
            node.shadow_including_parent()
        } else {
            Node::composed_parent(node)
        }
    };
    std::iter::successors(Some(node.clone()), parent).any(|node| {
        styles
            .styles
            .get(&node.id())
            .is_some_and(|style| style.display == Display::None)
    })
}

#[cfg(test)]
mod tests;
