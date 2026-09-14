//! Classic scrollbar gutters participate in sizing, not just painting.
//! https://drafts.csswg.org/css-overflow-3/#scrollbar-layout
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::engine::layout) fn layout_block_with_content_height(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        height: Option<f32>,
        used_width: Option<UsedInlineSize>,
        used_height: Option<f32>,
    ) -> BlockMetrics {
        let axes = self.styles.get(node).overflow_axes();
        if matches!(node.tag_name(), Some("html" | "body"))
            || ![axes.0, axes.1]
                .iter()
                .any(|axis| matches!(axis, Overflow::Auto | Overflow::Scroll))
        {
            let result =
                self.layout_block_attempt(node, x, y, width, height, used_width, used_height);
            self.commit_scroll_offset(node);
            return result;
        }
        self.scroll_gutters.insert(
            node.id(),
            (axes.0 == Overflow::Scroll, axes.1 == Overflow::Scroll),
        );
        let item_start = self.output.items.len();
        let node_start = self.output.node_paint_order.len();
        let flow_start = self.positioned_flow_scopes.last().map_or(0, Vec::len);
        let floats = self.floats.clone();
        // Adding one gutter can induce overflow on the other axis. At most two
        // monotonic additions are needed; never oscillate auto bars within a layout.
        for attempt in 0..3 {
            let result =
                self.layout_block_attempt(node, x, y, width, height, used_width, used_height);
            let Some(scroll) = self.output.scroll_boxes.get(&node.id()) else {
                return result;
            };
            let required = (scroll.bar_x, scroll.bar_y);
            if self.scroll_gutters[&node.id()] == required || attempt == 2 {
                self.commit_scroll_offset(node);
                return result;
            }
            self.scroll_gutters.insert(node.id(), required);
            self.output.items.truncate(item_start);
            self.output.node_paint_order.truncate(node_start);
            if let Some(flow) = self.positioned_flow_scopes.last_mut() {
                flow.truncate(flow_start);
            }
            self.floats = floats.clone();
            for child in Node::composed_descendants(node) {
                self.output.node_bounds.remove(&child.id());
                self.output.resize_boxes.remove(&child.id());
                self.output.scroll_boxes.remove(&child.id());
            }
        }
        unreachable!("each scrollbar axis can be added only once")
    }

    fn commit_scroll_offset(&self, node: &NodeRef) {
        // Intermediate gutter retries and CSSOM geometry probes must not clamp
        // away a position that is reachable in the final layout.
        if self.emit_paint
            && let Some(scroll) = self.output.scroll_boxes.get(&node.id())
        {
            node.scroll_offset.set((scroll.offset_x, scroll.offset_y));
        }
    }
}
