//! Block position resolution for normal, floating, relative, absolute, and fixed boxes.

use super::super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn layout_positioned_children(
        &mut self,
        node: &NodeRef,
        containing_block: RectF,
        in_flow_paint_start: usize,
        in_flow_node_start: usize,
    ) {
        let mut groups = Vec::new();
        for (source_order, child) in self.box_children(node).into_iter().enumerate() {
            let child_style = self.styles.get(&child);
            if matches!(child_style.position, Position::Absolute | Position::Fixed)
                && child_style.display != Display::None
                && child_style.visibility
            {
                // CSS 2.1 section 10.1: a positioned block establishes its padding box as the
                // containing block for absolutely positioned descendants. Fixed boxes still
                // select the initial containing block in resolve_block_position.
                let item_start = self.output.items.len();
                let node_start = self.output.node_paint_order.len();
                self.layout_block(
                    &child,
                    containing_block.x,
                    containing_block.y,
                    containing_block.width,
                    Some(containing_block.height),
                    None,
                );
                groups.push(PositionedPaintGroup {
                    level: child_style.z_index.unwrap_or(0),
                    source_order,
                    items: self.output.items.split_off(item_start),
                    nodes: self.output.node_paint_order.split_off(node_start),
                });
            }
        }

        // CSS 2.1 section 9.9 and Appendix E: integer levels sort numerically, while `auto`
        // participates at level zero. Stable source order resolves equal levels. Keeping a
        // descendant's complete item sequence together preserves nested opacity and transform
        // groups as an atomic stacking unit.
        groups.sort_by_key(|group| (group.level, group.source_order));
        let first_non_negative = groups.partition_point(|group| group.level < 0);
        let mut negative_items = Vec::new();
        let mut negative_nodes = Vec::new();
        for group in groups.drain(..first_non_negative) {
            negative_items.extend(group.items);
            negative_nodes.extend(group.nodes);
        }
        self.output
            .items
            .splice(in_flow_paint_start..in_flow_paint_start, negative_items);
        self.output
            .node_paint_order
            .splice(in_flow_node_start..in_flow_node_start, negative_nodes);
        for group in groups {
            self.output.items.extend(group.items);
            self.output.node_paint_order.extend(group.nodes);
        }
    }

    pub(super) fn resolve_block_position(
        &self,
        style: &ComputedStyle,
        containing_x: f32,
        y: f32,
        containing_width: f32,
        containing_height: Option<f32>,
        margins: ResolvedEdges,
        border_box_width: f32,
    ) -> (f32, f32) {
        let auto_left = style.margin.left == Length::Auto;
        let auto_right = style.margin.right == Length::Auto;
        let mut x = containing_x + margins.left;
        if auto_left && auto_right && border_box_width < containing_width {
            x = containing_x + (containing_width - border_box_width) / 2.0;
        } else if style.float == Float::Right || auto_left {
            x = containing_x + containing_width - border_box_width - margins.right;
        }

        let mut border_y = y + margins.top;
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            let (positioning_x, positioning_y, positioning_width, positioning_height) =
                if style.position == Position::Fixed {
                    (
                        self.viewport.x,
                        self.viewport.y,
                        self.viewport.width,
                        self.viewport.height,
                    )
                } else {
                    (
                        containing_x,
                        y,
                        containing_width,
                        containing_height.unwrap_or(self.viewport.height),
                    )
                };
            let left = style.left.resolve(positioning_width, style.font_size);
            let right = style.right.resolve(positioning_width, style.font_size);
            if let Some(left) = left {
                x = positioning_x + left + margins.left;
                if right.is_some()
                    && auto_left
                    && auto_right
                    && border_box_width < positioning_width
                {
                    x = positioning_x + (positioning_width - border_box_width) / 2.0;
                }
            } else if let Some(right) = right {
                x = positioning_x + positioning_width - border_box_width - right - margins.right;
            }
            if let Some(top) = style.top.resolve(positioning_height, style.font_size) {
                border_y = positioning_y + top + margins.top;
            } else if let Some(bottom) = style.bottom.resolve(positioning_height, style.font_size) {
                border_y = positioning_y + positioning_height - bottom;
            }
        } else if style.position == Position::Relative {
            if let Some(left) = style.left.resolve(containing_width, style.font_size) {
                x += left;
            } else if let Some(right) = style.right.resolve(containing_width, style.font_size) {
                x -= right;
            }
            if let Some(top) = style.top.resolve(self.viewport.height, style.font_size) {
                border_y += top;
            } else if let Some(bottom) = style.bottom.resolve(self.viewport.height, style.font_size)
            {
                border_y -= bottom;
            }
        }
        (x, border_y)
    }
}

struct PositionedPaintGroup {
    level: i32,
    source_order: usize,
    items: Vec<DisplayItem>,
    nodes: Vec<NodeId>,
}
