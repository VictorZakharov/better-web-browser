mod column;
mod flexible_lengths;
mod flow;
mod intrinsic;
mod item;

use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct CrossConstraints {
    pub minimum: f32,
    pub maximum: Option<f32>,
}

impl CrossConstraints {
    fn clamp(self, size: f32) -> f32 {
        size.min(self.maximum.unwrap_or(f32::INFINITY))
            .max(self.minimum)
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn layout_flex(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        containing_height: Option<f32>,
        cross: CrossConstraints,
        style: &ComputedStyle,
    ) -> f32 {
        let composed_children = self.box_children(node);
        let mut items = Vec::new();
        let mut anonymous_atoms = Vec::new();
        let mut pending_space = None;
        for child in composed_children {
            if child.element().is_none() {
                if matches!(&child.data, NodeData::Text(text) | NodeData::Cdata(text) if !text.borrow().trim().is_empty())
                {
                    self.collect_inline_root(
                        &child,
                        &mut anonymous_atoms,
                        &mut pending_space,
                        false,
                        InlineContainingBlock {
                            width,
                            height: containing_height,
                        },
                    );
                }
                continue;
            }
            if !anonymous_atoms.is_empty() {
                let atoms = std::mem::take(&mut anonymous_atoms);
                items.push(self.anonymous_flex_item(atoms, width));
                pending_space = None;
            }
            let child_style = self.styles.get(&child).clone();
            if child_style.display == Display::None
                || style_collapses_overflow(&child_style, self.viewport)
            {
                continue;
            }
            if matches!(child_style.position, Position::Absolute | Position::Fixed) {
                // Positioned children are installed by layout_block after the flex
                // container's padding-box block size is known.
                continue;
            } else {
                let child_style = self.styles.get(&child).clone();
                items.push(FlexItem {
                    basis: if style.flex_direction.is_row() {
                        self.flex_item_basis(&child, &child_style, width)
                    } else {
                        // In a column, flex-basis is a height, not a width.
                        let mut cross_style = child_style.clone();
                        cross_style.flex_basis = Length::Auto;
                        self.flex_item_basis(&child, &cross_style, width)
                    },
                    grow: child_style.flex_grow,
                    shrink: child_style.flex_shrink,
                    margin_start_auto: child_style.margin.left == Length::Auto,
                    margin_end_auto: child_style.margin.right == Length::Auto,
                    node: Some(child),
                    anonymous_atoms: Vec::new(),
                });
            }
        }
        if !anonymous_atoms.is_empty() {
            items.push(self.anonymous_flex_item(anonymous_atoms, width));
        }
        if items.is_empty() {
            return y;
        }

        match style.flex_direction {
            FlexDirection::Column | FlexDirection::ColumnReverse => {
                self.layout_flex_column(&items, x, y, width, containing_height, style)
            }
            FlexDirection::Row | FlexDirection::RowReverse => {
                self.layout_flex_rows(&items, x, y, width, containing_height, cross, style)
            }
        }
    }

    pub(super) fn flex_item_basis(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        available_width: f32,
    ) -> f32 {
        let margin = style.margin.resolve(available_width, style.font_size);
        let border = table::resolved_table_borders(node, style, available_width);
        let padding = style.padding.resolve(available_width, style.font_size);
        let insets = border.horizontal() + padding.horizontal();
        let specified = if style.flex_basis != Length::Auto {
            resolve_outer_size(
                style.flex_basis.clone(),
                available_width,
                style.font_size,
                insets,
                style.box_sizing,
            )
        } else {
            resolve_outer_size(
                style.width.clone(),
                available_width,
                style.font_size,
                insets,
                style.box_sizing,
            )
        };
        let intrinsic_width = if specified.is_some() {
            0.0
        } else if matches!(style.display, Display::Flex | Display::InlineFlex) {
            self.flex_container_intrinsic_width(node, style, Some(available_width))
        } else {
            self.block_container_intrinsic_width(node, Some(available_width))
        };
        let mut basis =
            specified.unwrap_or(intrinsic_width + insets).max(0.0) + margin.horizontal();
        if let Some(minimum) = resolve_outer_size(
            style.min_width.clone(),
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            basis = basis.max(minimum + margin.horizontal());
        }
        if let Some(maximum) = resolve_outer_size(
            style.max_width.clone(),
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            basis = basis.min(maximum + margin.horizontal());
        }
        basis
    }
}
