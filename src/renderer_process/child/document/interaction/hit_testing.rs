//! Renderer-owned element targeting and clipping-aware visual hit testing.
use super::*;
impl DocumentRuntime {
    pub(super) fn resolve_target(&self, target: DocumentNodeId) -> Option<NodeRef> {
        NodeId::from_wire(target.get()).and_then(|id| self.page.dom.find_node(id))
    }

    pub(super) fn explicit_target(&self, target: DocumentNodeId) -> Option<HitTarget> {
        let node = self.resolve_target(target)?;
        self.layout.items.iter().find_map(|item| match item {
            DisplayItem::Control(control) if control.node_id == node.id() => Some(HitTarget {
                node: node.clone(),
                link: None,
                control: Some((**control).clone()),
            }),
            DisplayItem::Text {
                node_id: Some(node_id),
                link: Some(link),
                ..
            } if *node_id == node.id() => Some(HitTarget {
                node: node.clone(),
                link: Some(link.clone()),
                control: None,
            }),
            _ => None,
        })
    }

    pub(super) fn hit_target(&self, x: f32, y: f32) -> Option<HitTarget> {
        self.layout
            .items
            .iter()
            .rev()
            .find_map(|item| match item {
                DisplayItem::Text {
                    rect,
                    link: Some(link),
                    node_id: Some(node_id),
                    ..
                } if contains(*rect, x, y) => self
                    .page
                    .dom
                    .find_node(*node_id)
                    .filter(|node| self.layout.point_in_scroll_clips(node, x, y))
                    .map(|node| HitTarget {
                        node,
                        link: Some(link.clone()),
                        control: None,
                    }),
                DisplayItem::Control(control) if contains(control.rect, x, y) => self
                    .page
                    .dom
                    .find_node(control.node_id)
                    .filter(|node| self.layout.point_in_scroll_clips(node, x, y))
                    .map(|node| HitTarget {
                        node,
                        link: None,
                        control: Some((**control).clone()),
                    }),
                _ => None,
            })
            .or_else(|| self.hit_element_bounds(x, y))
    }

    fn hit_element_bounds(&self, x: f32, y: f32) -> Option<HitTarget> {
        self.layout
            .node_paint_order
            .iter()
            .rev()
            .find_map(|id| {
                let node = self.page.dom.find_node(*id)?;
                let rect = self.layout.visual_rect(&node)?;
                (rect.width > 0.0
                    && rect.height > 0.0
                    && contains(rect, x, y)
                    && self.layout.point_in_scroll_clips(&node, x, y))
                .then_some(node)
            })
            .map(|node| HitTarget {
                node,
                link: None,
                control: None,
            })
    }
}
