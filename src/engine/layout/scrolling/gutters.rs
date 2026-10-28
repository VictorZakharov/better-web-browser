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
        let height = self.read_height_basis(node, height);
        // An unchanged subtree replays the layout an earlier pass retained.
        let key = self.subtree_key(node, width, height, used_width, used_height);
        if let Some(metrics) = self.replay_subtree(&key, node, x, y) {
            return metrics;
        }
        if let Some(metrics) = self.replay_measured(&key, node, x, y) {
            return metrics;
        }
        self.page.count_layout(|stats| stats.blocks_laid_out += 1);
        // This layout, not an earlier replay in the same pass, places the subtree.
        self.replayed_roots.remove(&node.id());
        let measured = self.measures_alone(node);
        if measured {
            self.begin_reads();
        }
        let capture = self.begin_layout_capture(key, node, x, y);
        let metrics =
            self.layout_block_with_gutters(node, x, y, width, height, used_width, used_height);
        if let Some(capture) = capture {
            self.finish_capture(capture, node, metrics);
        }
        if measured {
            self.remember_measured(key, x, y, metrics);
        }
        metrics
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_block_with_gutters(
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
            || self
                .styles
                .get(node)
                .scrollbar_width
                .thickness(self.page.scrollbar_thickness())
                == 0.0
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
                self.output.transformed.remove(child.id());
                std::sync::Arc::make_mut(&mut self.output.fragments).remove(child.id());
                self.output.node_bounds.remove(&child.id());
                self.output.resize_boxes.remove(&child.id());
                self.output.scroll_boxes.remove(&child.id());
            }
        }
        unreachable!("each scrollbar axis can be added only once")
    }

    fn commit_scroll_offset(&self, node: &NodeRef) {
        // Intermediate gutter retries, CSSOM geometry probes and block-size probes that paint
        // must not clamp away a position that is reachable in the final layout.
        if self.emit_paint
            && self.probe_root.is_none()
            && let Some(scroll) = self.output.scroll_boxes.get(&node.id())
        {
            node.scroll_offset.set((scroll.offset_x, scroll.offset_y));
        }
    }
}
