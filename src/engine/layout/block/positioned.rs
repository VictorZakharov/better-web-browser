//! Block position resolution for normal, floating, relative, absolute, and fixed boxes.

use super::super::*;
#[cfg(test)]
#[path = "positioned_tests.rs"]
mod tests;

pub(super) fn containing_box(style: &ComputedStyle, padding_box: RectF, inherited: RectF) -> RectF {
    if establishes_containing_block(style) {
        padding_box
    } else {
        // Static boxes without a transform do not establish a containing block.
        inherited
    }
}

fn establishes_containing_block(style: &ComputedStyle) -> bool {
    style.position != Position::Static || style.has_transform()
}

/// The containing block of absolutely positioned descendants: the padding box of the nearest
/// ancestor that establishes one, or the initial containing block when `block` is `None`.
/// `depth` counts those ancestors, which tells a retained subtree whether the containing block
/// of a box inside it was established inside it (`note_containing_block`).
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::engine::layout) struct AbsoluteScope {
    block: Option<RectF>,
    depth: usize,
}

impl AbsoluteScope {
    pub(in crate::engine::layout) fn depth(self) -> usize {
        self.depth
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// CSS 2.1 §10.1: absolute descendants of static boxes use the nearest positioned
    /// ancestor's padding box, or the initial containing block. Descendants are laid out
    /// before an auto-height ancestor's height is known, so the caller supplies the best
    /// available padding box (specified height, else the ancestor's own containing height).
    pub(super) fn enter_absolute_scope(
        &mut self,
        style: &ComputedStyle,
        padding_box: RectF,
    ) -> AbsoluteScope {
        let previous = self.absolute_scope;
        if establishes_containing_block(style) {
            self.absolute_scope = AbsoluteScope {
                block: Some(padding_box),
                depth: previous.depth + 1,
            };
        }
        previous
    }

    /// Restores the enclosing scope. Returns its containing block, and the depth of the one this
    /// box's absolutely positioned children resolve against: the innermost scope, whether this
    /// box established it or not.
    pub(super) fn exit_absolute_scope(&mut self, previous: AbsoluteScope) -> (RectF, usize) {
        let depth = self.absolute_scope.depth;
        self.absolute_scope = previous;
        (previous.block.unwrap_or(self.viewport), depth)
    }

    pub(super) fn finish_positioned_flow_scope(
        &mut self,
        node: NodeId,
        style: &ComputedStyle,
        item_start: usize,
        node_start: usize,
    ) {
        if !self.emit_paint {
            return;
        }
        self.positioned_flow_scopes.pop();
        if matches!(style.position, Position::Relative | Position::Sticky)
            && let Some(parent) = self.positioned_flow_scopes.last_mut()
        {
            parent.push(engine::InFlowPaintRange {
                node,
                level: style.z_index.unwrap_or(0),
                items: item_start..self.output.items.len(),
                nodes: node_start..self.output.node_paint_order.len(),
            });
        }
    }

    /// Lays out `node`'s absolutely positioned children against `containing_block`, the scope
    /// at the given depth (`AbsoluteScope`).
    pub(in crate::engine::layout) fn layout_positioned_children(
        &mut self,
        node: &NodeRef,
        (containing_block, containing_depth): (RectF, usize),
        static_position: (f32, f32),
        in_flow_paint_start: usize,
        in_flow_node_start: usize,
    ) {
        let children = self.positioned_candidates(node);
        let mut groups = Vec::new();
        // In-flow geometry is already complete (including flex/grid alignment). Extract
        // positioned paint ranges only now so deferring paint cannot change sizing.
        let mut in_flow = self
            .positioned_flow_scopes
            .last_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        in_flow.sort_by_key(|group| group.items.start);
        for group in in_flow.into_iter().rev() {
            groups.push(PositionedPaintGroup {
                level: group.level,
                source_order: children
                    .iter()
                    .position(|child| child.id() == group.node)
                    .unwrap_or(usize::MAX),
                items: self.output.items.drain(group.items).collect(),
                nodes: self.output.node_paint_order.drain(group.nodes).collect(),
            });
        }
        for (source_order, child) in children.into_iter().enumerate() {
            let child_style = self.styles.get(&child);
            if matches!(child_style.position, Position::Absolute | Position::Fixed)
                && child_style.display != Display::None
            {
                // CSS 2.1 section 10.1: a positioned block establishes its padding box as the
                // containing block for absolutely positioned descendants. Fixed boxes still
                // select the initial containing block in resolve_block_position.
                let item_start = self.output.items.len();
                let node_start = self.output.node_paint_order.len();
                let containing_block = if child_style.position == Position::Fixed {
                    self.viewport
                } else {
                    self.note_containing_block(containing_depth);
                    containing_block
                };
                self.layout_block(
                    &child,
                    containing_block.x,
                    containing_block.y,
                    containing_block.width,
                    Some(containing_block.height),
                    None,
                );
                // When both insets on an axis are auto, the hypothetical in-flow
                // position supplies that axis. Definite insets still resolve from
                // the actual containing block, which can differ for fixed boxes.
                // https://www.w3.org/TR/CSS22/visudet.html#abs-non-replaced-width
                let dx = if child_style.left == Length::Auto && child_style.right == Length::Auto {
                    static_position.0 - containing_block.x
                } else {
                    0.0
                };
                let dy = if child_style.top == Length::Auto && child_style.bottom == Length::Auto {
                    static_position.1 - containing_block.y
                } else {
                    0.0
                };
                if dx != 0.0 || dy != 0.0 {
                    let item_end = self.output.items.len();
                    self.translate_layout_subtree(Some(&child), item_start, item_end, dx, dy);
                }
                if self.emit_paint {
                    groups.push(PositionedPaintGroup {
                        level: child_style.z_index.unwrap_or(0),
                        source_order,
                        items: self.output.items.split_off(item_start),
                        nodes: self.output.node_paint_order.split_off(node_start),
                    });
                }
            }
        }

        if !self.emit_paint {
            return;
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

    /// Box children plus the absolutely positioned boxes nested in static inline children
    /// that have an inset on both axes, in tree order. A non-positioned inline box establishes no containing block, so those
    /// descendants belong to this block's positioning scope (CSS 2.1 §10.1).
    fn positioned_candidates(&self, node: &NodeRef) -> Vec<NodeRef> {
        fn nested<M: TextMeasurer>(
            engine: &LayoutEngine<'_, M>,
            inline: &NodeRef,
            output: &mut Vec<NodeRef>,
        ) {
            let style = engine.styles.get(inline);
            if style.display != Display::Inline
                || style.position != Position::Static
                || style.has_transform()
                || inline.represents_content_navigable()
            {
                return;
            }
            for child in engine.box_children(inline) {
                let child_style = engine.styles.get(&child);
                if matches!(child_style.position, Position::Absolute | Position::Fixed) {
                    // The static position inside a line box is not recorded, so only boxes
                    // that never use it (an inset on each axis) are laid out from here.
                    let placed = |start: &Length, end: &Length| {
                        *start != Length::Auto || *end != Length::Auto
                    };
                    if placed(&child_style.left, &child_style.right)
                        && placed(&child_style.top, &child_style.bottom)
                    {
                        output.push(child);
                    }
                } else {
                    nested(engine, &child, output);
                }
            }
        }
        let mut candidates = Vec::new();
        for child in self.box_children(node) {
            candidates.push(child.clone());
            nested(self, &child, &mut candidates);
        }
        candidates
    }

    #[allow(clippy::too_many_arguments)]
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

pub(super) fn bottom_alignment_shift(
    style: &ComputedStyle,
    containing_height: f32,
    border_box_height: f32,
    margin_bottom: f32,
) -> f32 {
    // With an automatic top inset, bottom constrains the bottom margin edge. Resolve
    // the used height first (including auto/min/max sizing), then solve for the top.
    // https://www.w3.org/TR/CSS22/visudet.html#abs-non-replaced-height
    if matches!(style.position, Position::Absolute | Position::Fixed)
        && style.top == Length::Auto
        && style
            .bottom
            .resolve(containing_height, style.font_size)
            .is_some()
    {
        -border_box_height - margin_bottom
    } else {
        0.0
    }
}

pub(super) fn popover_center_shift(
    node: &NodeRef,
    style: &ComputedStyle,
    containing_height: f32,
    border_box_height: f32,
) -> f32 {
    if !node.is_popover_open()
        || style.position != Position::Fixed
        || style.margin.top != Length::Auto
        || style.margin.bottom != Length::Auto
    {
        return 0.0;
    }
    let (Some(top), Some(bottom)) = (
        style.top.resolve(containing_height, style.font_size),
        style.bottom.resolve(containing_height, style.font_size),
    ) else {
        return 0.0;
    };
    // CSS positioned layout splits positive free space between automatic margins.
    (containing_height - top - bottom - border_box_height).max(0.0) / 2.0
}
