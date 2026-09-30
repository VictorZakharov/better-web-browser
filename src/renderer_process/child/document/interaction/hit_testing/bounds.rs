//! A query-owned DOM lookup for native plain-element hit testing.
use crate::engine::LayoutOutput;
use crate::engine::dom::{Dom, NodeId, NodeRef};
use std::collections::HashMap;

#[cfg(test)]
mod tests;

struct CurrentNodes {
    nodes: HashMap<NodeId, NodeRef>,
    #[cfg(test)]
    indexed_nodes: usize,
    #[cfg(test)]
    candidate_lookups: usize,
}

impl CurrentNodes {
    fn new(document: &NodeRef) -> Self {
        let mut result = Self {
            nodes: HashMap::new(),
            #[cfg(test)]
            indexed_nodes: 0,
            #[cfg(test)]
            candidate_lookups: 0,
        };
        // Match Dom::find_node ownership, not composed-tree/CSSOM hit-test policy:
        // closed shadow roots and template contents are searchable, adopted IDs
        // retain their allocation namespace, and detached/generated nodes are not.
        // Rebuild for this query: template-content mutation need not advance the
        // current document's mutation version, so a generation cache is unsafe.
        let mut stack = vec![document.clone()];
        while let Some(node) = stack.pop() {
            #[cfg(test)]
            {
                result.indexed_nodes += 1;
            }
            stack.extend(node.children.borrow().iter().rev().cloned());
            if let Some(shadow) = node.shadow_root() {
                stack.push(shadow);
            }
            if let Some(contents) = node
                .element()
                .and_then(|element| element.template_contents.borrow().clone())
            {
                stack.push(contents);
            }
            result.nodes.entry(node.id()).or_insert(node);
        }
        result
    }

    fn hit(&mut self, layout: &LayoutOutput, x: f32, y: f32) -> Option<NodeRef> {
        layout.node_paint_order.iter().rev().find_map(|id| {
            #[cfg(test)]
            {
                self.candidate_lookups += 1;
            }
            let node = self.nodes.get(id)?;
            let rect = layout.visual_rect(node)?;
            (rect.width > 0.0
                && rect.height > 0.0
                && super::super::contains(rect, x, y)
                && !layout.hit_excluded.contains(&node.id())
                && layout.point_in_scroll_clips(node, x, y))
            .then(|| node.clone())
        })
    }
}

pub(super) fn node_at_point(dom: &Dom, layout: &LayoutOutput, x: f32, y: f32) -> Option<NodeRef> {
    CurrentNodes::new(&dom.document).hit(layout, x, y)
}
