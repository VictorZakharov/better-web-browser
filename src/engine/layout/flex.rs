mod flow;
mod item;

use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_flex(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        containing_height: Option<f32>,
        style: &ComputedStyle,
    ) -> f32 {
        let composed_children = self.box_children(node);
        let mut items = Vec::new();
        let mut anonymous_atoms = Vec::new();
        let mut pending_space = false;
        for child in composed_children {
            if child.element().is_none() {
                if matches!(&child.data, NodeData::Text(text) if !text.borrow().trim().is_empty()) {
                    self.collect_inline(
                        &child,
                        None,
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
                pending_space = false;
            }
            let child_style = self.styles.get(&child).clone();
            if child_style.display == Display::None
                || !child_style.visibility
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
                    basis: self.flex_item_basis(&child, &child_style, width),
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
                self.layout_flex_rows(&items, x, y, width, containing_height, style)
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
                style.flex_basis,
                available_width,
                style.font_size,
                insets,
                style.box_sizing,
            )
        } else {
            resolve_outer_size(
                style.width,
                available_width,
                style.font_size,
                insets,
                style.box_sizing,
            )
        };
        let intrinsic_width = if specified.is_some() {
            0.0
        } else if matches!(style.display, Display::Flex | Display::InlineFlex) {
            self.flex_container_intrinsic_width(node, style, available_width)
        } else {
            self.block_container_intrinsic_width(node, available_width)
        };
        let mut basis =
            specified.unwrap_or(intrinsic_width + insets).max(0.0) + margin.horizontal();
        if let Some(minimum) = resolve_outer_size(
            style.min_width,
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            basis = basis.max(minimum + margin.horizontal());
        }
        if let Some(maximum) = resolve_outer_size(
            style.max_width,
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            basis = basis.min(maximum + margin.horizontal());
        }
        basis
    }

    fn block_container_intrinsic_width(&mut self, node: &NodeRef, available_width: f32) -> f32 {
        let mut widest = 0.0_f32;
        let mut inline_atoms = Vec::new();
        let mut pending_space = false;
        for child in self.box_children(node) {
            let child_style = self.styles.get(&child).clone();
            if child.element().is_some()
                && is_block_level(child_style.display)
                && !matches!(child_style.position, Position::Absolute | Position::Fixed)
            {
                if !inline_atoms.is_empty() {
                    widest = widest.max(self.inline_intrinsic_width(
                        &std::mem::take(&mut inline_atoms),
                        available_width,
                    ));
                    pending_space = false;
                }
                widest = widest.max(self.flex_item_basis(&child, &child_style, available_width));
            } else {
                self.collect_inline(
                    &child,
                    None,
                    &mut inline_atoms,
                    &mut pending_space,
                    false,
                    InlineContainingBlock {
                        width: available_width,
                        height: None,
                    },
                );
            }
        }
        if !inline_atoms.is_empty() {
            widest = widest.max(self.inline_intrinsic_width(&inline_atoms, available_width));
        }
        widest
    }

    fn flex_container_intrinsic_width(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        available_width: f32,
    ) -> f32 {
        let mut contributions = Vec::new();
        let mut anonymous_atoms = Vec::new();
        let mut pending_space = false;
        for child in self.box_children(node) {
            if child.element().is_none() {
                if matches!(&child.data, NodeData::Text(text) if !text.borrow().trim().is_empty()) {
                    self.collect_inline(
                        &child,
                        None,
                        &mut anonymous_atoms,
                        &mut pending_space,
                        false,
                        InlineContainingBlock {
                            width: available_width,
                            height: None,
                        },
                    );
                }
                continue;
            }
            if !anonymous_atoms.is_empty() {
                contributions.push(self.inline_intrinsic_width(
                    &std::mem::take(&mut anonymous_atoms),
                    available_width,
                ));
                pending_space = false;
            }
            let child_style = self.styles.get(&child).clone();
            if child_style.display == Display::None
                || !child_style.visibility
                || matches!(child_style.position, Position::Absolute | Position::Fixed)
                || style_collapses_overflow(&child_style, self.viewport)
            {
                continue;
            }
            contributions.push(self.flex_item_max_content_contribution(
                &child,
                &child_style,
                available_width,
            ));
        }
        if !anonymous_atoms.is_empty() {
            contributions.push(self.inline_intrinsic_width(&anonymous_atoms, available_width));
        }
        let gap = style
            .grid_column_gap
            .resolve(available_width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        if style.flex_direction.is_row() {
            contributions.iter().sum::<f32>() + gap * contributions.len().saturating_sub(1) as f32
        } else {
            contributions.into_iter().fold(0.0, f32::max)
        }
    }

    /// CSS Flexbox 9.9.3 defines a flex item's max-content contribution independently from
    /// its flex base size. In particular, `flex: 1 1 0%` contributes its contents when an
    /// auto-sized ancestor asks for an intrinsic width; treating the zero basis as the
    /// contribution collapses common icon-and-label controls before flexing can occur.
    fn flex_item_max_content_contribution(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        available_width: f32,
    ) -> f32 {
        let margin = style.margin.resolve(available_width, style.font_size);
        let border = style.border_width.resolve(available_width, style.font_size);
        let padding = style.padding.resolve(available_width, style.font_size);
        let insets = border.horizontal() + padding.horizontal();
        // Percentage preferred sizes are indefinite under a max-content constraint. Passing the
        // actual containing width here incorrectly turns every nested `width: 100%` wrapper into
        // another full viewport contribution. A zero percentage basis preserves fixed lengths
        // and content while treating those percentages as zero for this intrinsic pass.
        let intrinsic_basis = 0.0;
        let intrinsic = if matches!(style.display, Display::Flex | Display::InlineFlex) {
            self.flex_container_intrinsic_width(node, style, intrinsic_basis)
        } else {
            self.block_container_intrinsic_width(node, intrinsic_basis)
        } + insets;
        let preferred = resolve_outer_size(
            style.width,
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        )
        .unwrap_or(0.0);
        let mut contribution = intrinsic.max(preferred);
        if let Some(minimum) = resolve_outer_size(
            style.min_width,
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            contribution = contribution.max(minimum);
        }
        if let Some(maximum) = resolve_outer_size(
            style.max_width,
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            contribution = contribution.min(maximum);
        }
        contribution.max(0.0) + margin.horizontal()
    }

    fn anonymous_flex_item(&mut self, atoms: Vec<InlineAtom>, available_width: f32) -> FlexItem {
        FlexItem {
            basis: self.inline_intrinsic_width(&atoms, available_width),
            grow: 0.0,
            shrink: 1.0,
            margin_start_auto: false,
            margin_end_auto: false,
            node: None,
            anonymous_atoms: atoms,
        }
    }

    fn inline_intrinsic_width(&mut self, atoms: &[InlineAtom], available_width: f32) -> f32 {
        self.begin_inline_measurement_context();
        let mut widest = 0.0_f32;
        let mut current = 0.0_f32;
        let mut line_start = true;
        for atom in atoms {
            if matches!(atom, InlineAtom::Break) {
                widest = widest.max(current);
                current = 0.0;
                line_start = true;
            } else {
                current += self.measure_atom(atom, line_start, available_width).width;
                line_start = false;
            }
        }
        widest.max(current)
    }
}
