//! CSSOM offset geometry remains unscrolled; hit testing uses visual scroll translations.
use super::*;

impl LayoutOutput {
    pub fn visual_rect(&self, node: &NodeRef) -> Option<RectF> {
        let mut rect = *self.node_bounds.get(&node.id())?;
        if let Some((x, y)) = self.sticky_offsets.get(&node.id()) {
            rect.x += x;
            rect.y += y;
        }
        for parent in std::iter::successors(Node::composed_parent(node), Node::composed_parent) {
            if let Some((x, y)) = self.sticky_offsets.get(&parent.id()) {
                rect.x += x;
                rect.y += y;
            }
            if let Some(scroll) = self.scroll_boxes.get(&parent.id()) {
                rect.x -= scroll.offset_x;
                rect.y -= scroll.offset_y;
            }
        }
        Some(rect)
    }

    pub fn point_in_scroll_clips(&self, node: &NodeRef, x: f32, y: f32) -> bool {
        for parent in std::iter::successors(Node::composed_parent(node), Node::composed_parent) {
            if let Some(scroll) = self.scroll_boxes.get(&parent.id()) {
                let Some(raw) = self.node_bounds.get(&parent.id()) else {
                    continue;
                };
                let Some(visual) = self.visual_rect(&parent) else {
                    continue;
                };
                let port = RectF {
                    x: scroll.port.x + visual.x - raw.x,
                    y: scroll.port.y + visual.y - raw.y,
                    ..scroll.port
                };
                if (scroll.clip_x && (x < port.x || x >= port.right()))
                    || (scroll.clip_y && (y < port.y || y >= port.bottom()))
                {
                    return false;
                }
            }
        }
        true
    }
}
