//! Retained paint-order geometry for CSSOM View point queries.
//! https://drafts.csswg.org/cssom-view/#dom-document-elementsfrompoint

use super::*;

/// A compact, renderer-local copy of the geometry needed for script hit testing.
/// Display items and glyphs stay with the painter.
#[derive(Debug, Clone, Default)]
pub struct HitTestSnapshot {
    layout: LayoutOutput,
    paint_nodes: Vec<NodeRef>,
    inline_nodes: Vec<NodeRef>,
}

impl HitTestSnapshot {
    pub fn from_layout(layout: &LayoutOutput, document: &NodeRef) -> Self {
        let ordered_nodes = Node::shadow_including_descendants(document).collect::<Vec<_>>();
        let nodes: HashMap<_, _> = ordered_nodes
            .iter()
            .cloned()
            .map(|node| (node.id(), node))
            .collect();
        let painted: std::collections::HashSet<_> =
            layout.node_paint_order.iter().copied().collect();
        let paint_nodes = layout
            .node_paint_order
            .iter()
            .filter_map(|id| nodes.get(id).cloned())
            .collect();
        let inline_nodes = ordered_nodes
            .into_iter()
            .filter(|node| {
                node.element().is_some()
                    && !painted.contains(&node.id())
                    && layout.node_bounds.contains_key(&node.id())
                    && layout.fragments.element(node.id()).is_some()
            })
            .collect();
        Self {
            layout: LayoutOutput {
                fragments: layout.fragments.clone(),
                node_bounds: layout.node_bounds.clone(),
                sticky_offsets: layout.sticky_offsets.clone(),
                scroll_boxes: layout.scroll_boxes.clone(),
                clip_paths: layout.clip_paths.clone(),
                hit_excluded: layout.hit_excluded.clone(),
                ..LayoutOutput::default()
            },
            paint_nodes,
            inline_nodes,
        }
    }

    /// Topmost-first painted elements under a point in document coordinates.
    /// The caller handles viewport bounds and the root-element fallback.
    pub fn elements_at(&self, x: f32, y: f32) -> Vec<NodeRef> {
        let mut hits = self
            .paint_nodes
            .iter()
            .rev()
            .filter(|node| {
                node.element().is_some()
                    && !self.layout.hit_excluded.contains(&node.id())
                    && self.layout.visual_rect(node).is_some_and(|rect| {
                        rect.width > 0.0
                            && rect.height > 0.0
                            && x >= rect.x
                            && x < rect.right()
                            && y >= rect.y
                            && y < rect.bottom()
                    })
                    && self.layout.point_in_scroll_clips(node, x, y)
            })
            .cloned()
            .collect::<Vec<_>>();
        // A text-only inline does not create a decoration marker in the paint
        // list, but its fragment is a hit-testable box. Insert such fragments
        // just above their nearest painted ancestor, retaining stacked boxes'
        // paint order. Reverse tree order makes later overlapping siblings win.
        for node in self.inline_nodes.iter().rev() {
            if self.layout.hit_excluded.contains(&node.id())
                || !self.layout.point_in_scroll_clips(node, x, y)
            {
                continue;
            }
            let Some(raw) = self.layout.node_bounds.get(&node.id()) else {
                continue;
            };
            let Some(visual) = self.layout.visual_rect(node) else {
                continue;
            };
            let Some(fragments) = self.layout.fragments.element(node.id()) else {
                continue;
            };
            if !fragments.iter().any(|rect| {
                let left = rect.x + visual.x - raw.x;
                let top = rect.y + visual.y - raw.y;
                rect.width > 0.0
                    && rect.height > 0.0
                    && x >= left
                    && x < left + rect.width
                    && y >= top
                    && y < top + rect.height
            }) {
                continue;
            }
            let ancestor_index =
                std::iter::successors(Node::composed_parent(node), Node::composed_parent)
                    .find_map(|ancestor| hits.iter().position(|hit| hit.id() == ancestor.id()));
            if let Some(index) = ancestor_index {
                hits.insert(index, node.clone());
            }
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

    #[test]
    fn inherited_pointer_events_none_skips_boxes_but_auto_descendant_can_hit() {
        let page = Page::parse(
            "<style>body{margin:0}main{width:100px;height:100px;pointer-events:none}div{width:50px;height:50px;pointer-events:auto}</style><main><div></div></main>",
            "https://example.test/",
        );
        let main = page.dom.elements_named("main").next().unwrap();
        let child = page.dom.elements_named("div").next().unwrap();
        let layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        assert!(layout.hit_excluded.contains(&main.id()));
        assert!(!layout.hit_excluded.contains(&child.id()));
        let hits =
            HitTestSnapshot::from_layout(&layout, &page.dom.document).elements_at(20.0, 20.0);
        assert_eq!(hits.first().map(|node| node.id()), Some(child.id()));
        assert!(!hits.iter().any(|node| node.id() == main.id()));
    }

    #[test]
    fn table_cell_background_is_a_hit_testable_box() {
        let page = Page::parse(
            "<style>body{margin:0}table{margin:100px;width:200px;height:200px}td{background:blue}</style><table><tr><td id=first></td><td id=second></td><td id=third></td><td id=fourth></td></tr></table>",
            "https://example.test/",
        );
        let cell = page.dom.elements_named("td").next().unwrap();
        let layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let rect = layout.node_bounds.get(&cell.id()).copied().unwrap();
        assert!(rect.x <= 125.0 && rect.right() > 125.0, "{rect:?}");
        assert!(rect.y <= 125.0 && rect.bottom() > 125.0, "{rect:?}");
        let hits =
            HitTestSnapshot::from_layout(&layout, &page.dom.document).elements_at(125.0, 125.0);
        assert_eq!(
            hits.first().map(|node| node.id()),
            Some(cell.id()),
            "{hits:?}"
        );
    }
}
