//! Intrinsic contributions for shrink-to-fit floats, independent of flex item sizing.
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn float_intrinsic_widths(
        &mut self,
        node: &NodeRef,
        percentage_basis: impl Into<Option<f32>>,
    ) -> (f32, f32) {
        let percentage_basis = percentage_basis.into();
        if let Some(widths) = self.intrinsic_widths.get(node.id(), percentage_basis, true) {
            return widths;
        }
        let basis = percentage_basis.unwrap_or(0.0);
        let style = self.styles.get(node).clone();
        let margin = style.margin.resolve(basis, style.font_size).horizontal();
        let insets = style.padding.resolve(basis, style.font_size).horizontal()
            + table::resolved_table_borders(node, &style, basis).horizontal();
        let specified = resolve_outer_size(
            intrinsic_constraint(style.width, percentage_basis, Length::Auto),
            basis,
            style.font_size,
            insets,
            style.box_sizing,
        );
        let (mut minimum, mut preferred) = if let Some(width) = specified {
            (width, width)
        } else if matches!(
            node.tag_name(),
            Some("img" | "image" | "video" | "svg" | "input" | "textarea" | "select")
        ) {
            let mut atoms = Vec::new();
            self.collect_inline(
                node,
                None,
                &mut atoms,
                &mut false,
                true,
                InlineContainingBlock {
                    width: basis,
                    height: None,
                },
            );
            let (lo, hi) = self.float_inline_widths(&atoms, basis);
            // Replaced/control atoms already include their own insets and margins.
            (lo - margin, hi - margin)
        } else {
            let (minimum, preferred) = self.intrinsic_content_widths(node, percentage_basis);
            (minimum + insets, preferred + insets)
        };
        if let Some(maximum) = resolve_outer_size(
            intrinsic_constraint(style.max_width, percentage_basis, Length::Auto),
            basis,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            minimum = minimum.min(maximum);
            preferred = preferred.min(maximum);
        }
        if let Some(lower) = resolve_outer_size(
            intrinsic_constraint(style.min_width, percentage_basis, Length::Px(0.0)),
            basis,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            minimum = minimum.max(lower);
            preferred = preferred.max(lower);
        }
        let widths = (minimum.max(0.0) + margin, preferred.max(0.0) + margin);
        self.intrinsic_widths
            .insert(node.id(), percentage_basis, true, widths);
        widths
    }

    pub(in crate::engine::layout) fn intrinsic_content_widths(
        &mut self,
        node: &NodeRef,
        percentage_basis: impl Into<Option<f32>>,
    ) -> (f32, f32) {
        let percentage_basis = percentage_basis.into();
        if let Some(widths) = self
            .intrinsic_widths
            .get(node.id(), percentage_basis, false)
        {
            return widths;
        }
        let basis = percentage_basis.unwrap_or(0.0);
        if self.styles.get(node).display == Display::Table {
            let widths = self.table_intrinsic_widths(node, basis);
            self.intrinsic_widths
                .insert(node.id(), percentage_basis, false, widths);
            return widths;
        }
        let mut minimum = 0.0_f32;
        let mut preferred = 0.0_f32;
        let mut atoms = Vec::new();
        let mut space = false;
        for child in self.block_formatting_children(node) {
            let child_style = self.styles.get(&child);
            if child_style.display == Display::None
                || matches!(child_style.position, Position::Absolute | Position::Fixed)
            {
                continue;
            }
            if is_block_level(child_style.display) {
                let (lo, hi) = self.float_inline_widths(&atoms, basis);
                minimum = minimum.max(lo);
                preferred = preferred.max(hi);
                atoms.clear();
                space = false;
                let (lo, hi) = self.float_intrinsic_widths(&child, percentage_basis);
                minimum = minimum.max(lo);
                preferred = preferred.max(hi);
            } else {
                self.collect_inline(
                    &child,
                    None,
                    &mut atoms,
                    &mut space,
                    true,
                    InlineContainingBlock {
                        width: basis,
                        height: None,
                    },
                );
            }
        }
        let (lo, hi) = self.float_inline_widths(&atoms, basis);
        let widths = (minimum.max(lo), preferred.max(hi));
        self.intrinsic_widths
            .insert(node.id(), percentage_basis, false, widths);
        widths
    }

    fn float_inline_widths(&mut self, atoms: &[InlineAtom], basis: f32) -> (f32, f32) {
        self.begin_inline_measurement_context();
        let mut minimum = 0.0_f32;
        let mut preferred = 0.0_f32;
        let mut run = 0.0_f32;
        let mut line = 0.0_f32;
        for atom in atoms {
            if matches!(atom, InlineAtom::Break) {
                minimum = minimum.max(run);
                preferred = preferred.max(line);
                run = 0.0;
                line = 0.0;
                continue;
            }
            let measured = self.measure_atom(atom, line == 0.0, basis);
            if measured.break_before && !measured.no_wrap {
                minimum = minimum.max(run);
                run = self.measure_atom(atom, true, basis).width;
            } else {
                run += measured.width;
            }
            line += measured.width;
        }
        (minimum.max(run), preferred.max(line))
    }
}

fn intrinsic_constraint(length: Length, basis: Option<f32>, initial: Length) -> Length {
    // CSS Sizing 3 §5.2.1: cyclic percentages cannot inflate the ancestor whose
    // intrinsic size supplies their basis. Actual layout resolves them normally.
    if basis.is_none()
        && (matches!(length, Length::Percent(_))
            || matches!(length, Length::Calc { percent, .. } if percent != 0.0))
    {
        initial
    } else {
        length
    }
}
