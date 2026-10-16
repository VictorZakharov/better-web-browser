//! Intrinsic contributions use the same transformed text as final flex layout.
use super::super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// CSS Flexbox 9.9.1: a single-line row sums item min-content contributions;
    /// a wrapping row or column uses the largest contribution in the inline axis.
    pub(in crate::engine::layout) fn flex_container_min_content_width(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        percentage_basis: Option<f32>,
    ) -> f32 {
        let available_width = percentage_basis.unwrap_or(0.0);
        let mut contributions = Vec::new();
        let mut anonymous_atoms = Vec::new();
        let mut pending_space = None;
        for child in self.box_children(node) {
            if child.element().is_none() {
                if matches!(&child.data, NodeData::Text(text) | NodeData::Cdata(text) if !text.borrow().trim().is_empty())
                {
                    self.collect_inline_root(
                        &child,
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
                contributions.push(
                    self.float_inline_widths(
                        &std::mem::take(&mut anonymous_atoms),
                        percentage_basis,
                    )
                    .0,
                );
                pending_space = None;
            }
            let child_style = self.styles.get(&child);
            if child_style.display == Display::None
                || matches!(child_style.position, Position::Absolute | Position::Fixed)
                || style_collapses_overflow(child_style, self.viewport)
            {
                continue;
            }
            contributions.push(self.float_intrinsic_widths(&child, percentage_basis).0);
        }
        if !anonymous_atoms.is_empty() {
            contributions.push(
                self.float_inline_widths(&anonymous_atoms, percentage_basis)
                    .0,
            );
        }
        if style.flex_direction.is_row() && !style.flex_wrap {
            let gap = style
                .grid_column_gap
                .resolve(available_width, style.font_size)
                .unwrap_or(0.0)
                .max(0.0);
            contributions.iter().sum::<f32>() + gap * contributions.len().saturating_sub(1) as f32
        } else {
            contributions.into_iter().fold(0.0, f32::max)
        }
    }

    pub(super) fn block_container_intrinsic_width(
        &mut self,
        node: &NodeRef,
        percentage_basis: Option<f32>,
    ) -> f32 {
        if let Some(width) = self
            .intrinsic_widths
            .max_content(node.id(), percentage_basis)
        {
            return width;
        }
        let available_width = percentage_basis.unwrap_or(0.0);
        let mut widest = 0.0_f32;
        let mut inline_atoms = Vec::new();
        let mut pending_space = None;
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
                    pending_space = None;
                }
                // This is an intrinsic contribution, not flex main-size resolution.
                // A descendant's zero flex basis must not erase its content width.
                widest = widest.max(self.flex_item_max_content_contribution(
                    &child,
                    &child_style,
                    percentage_basis,
                ));
            } else {
                self.collect_inline_root(
                    &child,
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
        self.intrinsic_widths
            .insert_max_content(node.id(), percentage_basis, widest);
        widest
    }

    pub(in crate::engine::layout) fn flex_container_intrinsic_width(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        percentage_basis: Option<f32>,
    ) -> f32 {
        if let Some(width) = self
            .intrinsic_widths
            .max_content(node.id(), percentage_basis)
        {
            return width;
        }
        let available_width = percentage_basis.unwrap_or(0.0);
        let mut contributions = Vec::new();
        let mut anonymous_atoms = Vec::new();
        let mut pending_space = None;
        for child in self.box_children(node) {
            if child.element().is_none() {
                if matches!(&child.data, NodeData::Text(text) | NodeData::Cdata(text) if !text.borrow().trim().is_empty())
                {
                    self.collect_inline_root(
                        &child,
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
                pending_space = None;
            }
            let child_style = self.styles.get(&child).clone();
            if child_style.display == Display::None
                || matches!(child_style.position, Position::Absolute | Position::Fixed)
                || style_collapses_overflow(&child_style, self.viewport)
            {
                continue;
            }
            contributions.push(self.flex_item_max_content_contribution(
                &child,
                &child_style,
                percentage_basis,
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
        let width = if style.flex_direction.is_row() {
            contributions.iter().sum::<f32>() + gap * contributions.len().saturating_sub(1) as f32
        } else {
            contributions.into_iter().fold(0.0, f32::max)
        };
        self.intrinsic_widths
            .insert_max_content(node.id(), percentage_basis, width);
        width
    }

    /// CSS Flexbox 9.9.3 defines a flex item's max-content contribution independently from
    /// its flex base size. In particular, `flex: 1 1 0%` contributes its contents when an
    /// auto-sized ancestor asks for an intrinsic width; treating the zero basis as the
    /// contribution collapses common icon-and-label controls before flexing can occur.
    fn flex_item_max_content_contribution(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        percentage_basis: Option<f32>,
    ) -> f32 {
        let available_width = percentage_basis.unwrap_or(0.0);
        let margin = style.margin.resolve(available_width, style.font_size);
        let border = style.border_width.resolve(available_width, style.font_size);
        let padding = style.padding.resolve(available_width, style.font_size);
        let insets = border.horizontal() + padding.horizontal();
        // A descendant's percentage basis depends on this intrinsic result. Preserve that
        // indefiniteness: cyclic percentage maxima act as none, not a zero-width maximum
        // (CSS Sizing 3 section 5.2.1). Resolve them normally in the subsequent layout pass.
        let intrinsic_basis = None;
        let intrinsic = if matches!(style.display, Display::Flex | Display::InlineFlex) {
            self.flex_container_intrinsic_width(node, style, intrinsic_basis)
        } else {
            self.block_container_intrinsic_width(node, intrinsic_basis)
        } + insets;
        let preferred = resolve_outer_size(
            style.width.clone(),
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        )
        .unwrap_or(0.0);
        let mut contribution = intrinsic.max(preferred);
        if let Some(minimum) = resolve_outer_size(
            style.min_width.clone(),
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            contribution = contribution.max(minimum);
        }
        let maximum_width = if percentage_basis.is_none()
            && (matches!(style.max_width, Length::Percent(_))
                || matches!(style.max_width, Length::Calc { percent, .. } if percent != 0.0))
        {
            Length::Auto
        } else {
            style.max_width.clone()
        };
        if let Some(maximum) = resolve_outer_size(
            maximum_width,
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            contribution = contribution.min(maximum);
        }
        contribution.max(0.0) + margin.horizontal()
    }

    pub(super) fn anonymous_flex_item(
        &mut self,
        atoms: Vec<InlineAtom>,
        available_width: f32,
    ) -> FlexItem {
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

    pub(super) fn inline_intrinsic_width(
        &mut self,
        atoms: &[InlineAtom],
        available_width: f32,
    ) -> f32 {
        let transformed = super::super::inline_layout::text_transform::apply(atoms);
        let atoms = transformed.as_ref();
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
