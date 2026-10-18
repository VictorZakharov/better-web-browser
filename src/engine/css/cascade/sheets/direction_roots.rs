//! Bounded style roots for `:dir()` instead of whole-document recalculation.
//
// An element's directionality comes from its own `dir` attribute, from its parent, or, for
// `dir=auto` and `<bdi>`, from the first strong character among its light-tree descendants
// (`selector_match::linguistic`). A change at or below a node can therefore flip only the
// directionality of that node's auto-directionality inclusive ancestors and, through
// inheritance, of their subtrees. Recalculating the subtree of the outermost such ancestor's
// parent also covers sibling combinators that read its `:dir()` result; ancestor `:has()`
// anchors are found by the relational walk from that root.
// https://html.spec.whatwg.org/multipage/dom.html#the-directionality
use super::*;

#[cfg(test)]
mod tests;

/// Roots whose subtrees contain every element whose directionality `requested` changes can
/// flip, beyond the requested subtrees themselves.
pub(super) fn direction_roots(requested: &[NodeRef]) -> Vec<NodeRef> {
    requested
        .iter()
        .filter_map(|node| {
            let outermost = std::iter::successors(Some(node.clone()), |node| node.parent())
                .filter(has_auto_direction)
                .last()?;
            Some(
                outermost
                    .shadow_including_parent()
                    .unwrap_or_else(|| outermost.clone()),
            )
        })
        .collect()
}

/// Mirrors the auto branches of `element_direction`.
fn has_auto_direction(node: &NodeRef) -> bool {
    let html = node.namespace_uri() == Some("http://www.w3.org/1999/xhtml");
    match node
        .attr_ref("dir")
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("ltr" | "rtl") if html => false,
        Some("auto") if html => true,
        _ => node.tag_name() == Some("bdi"),
    }
}
