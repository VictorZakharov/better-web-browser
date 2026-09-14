use super::*;
mod boundaries;
mod fragment_collection;
pub(super) mod geometry;
mod wrapping;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_inline_atoms(
        &mut self,
        atoms: &[InlineAtom],
        x: f32,
        mut y: f32,
        width: f32,
        align: TextAlign,
        default_line_height: f32,
    ) -> f32 {
        self.begin_inline_measurement_context();
        let runs = self.unbreakable_run_widths(atoms, width);
        let mut line = Vec::new();
        let mut line_width = 0.0_f32;
        let mut line_height = 0.0_f32;
        let (mut line_x, mut available) = self.floats.band(x, y, width, default_line_height);

        for (index, atom) in atoms.iter().enumerate() {
            if matches!(atom, InlineAtom::Break) {
                y = self.paint_line(
                    &line,
                    line_x,
                    y,
                    available,
                    align,
                    line_width,
                    line_height.max(default_line_height),
                );
                line.clear();
                line_width = 0.0;
                line_height = 0.0;
                (line_x, available) = self.floats.band(x, y, width, default_line_height);
                continue;
            }
            let measured = self.measure_atom(atom, line.is_empty(), width);
            let run_width = runs[index];
            if line.is_empty() {
                (line_x, y, available) = self.floats.fit(
                    x,
                    y,
                    width,
                    run_width,
                    measured.height.max(default_line_height),
                );
            }
            let should_wrap = !line.is_empty()
                && line_width + run_width > available
                && self.inline_break_before(atoms, index);
            if should_wrap {
                y = self.paint_line(
                    &line,
                    line_x,
                    y,
                    available,
                    align,
                    line_width,
                    line_height.max(default_line_height),
                );
                line.clear();
                line_width = 0.0;
                line_height = 0.0;
                (line_x, y, available) = self.floats.fit(
                    x,
                    y,
                    width,
                    run_width,
                    measured.height.max(default_line_height),
                );
            }
            let measured = if should_wrap {
                self.measure_atom(atom, true, width)
            } else {
                measured
            };
            line_width += measured.width;
            line_height = line_height.max(measured.height);
            line.push(measured);
        }
        if !line.is_empty() {
            y = self.paint_line(
                &line,
                line_x,
                y,
                available,
                align,
                line_width,
                line_height.max(default_line_height),
            );
        }
        y
    }

    pub(super) fn begin_inline_measurement_context(&mut self) {
        // Inline atoms are short-lived per formatting context, so pointer-keyed measurements
        // must not outlive a context and alias recycled allocations from a later atom tree.
        self.measurement_cache.clear();
        self.inline_box_cache.clear();
    }

    pub(super) fn measure_atom<'a>(
        &mut self,
        atom: &'a InlineAtom,
        line_start: bool,
        containing_width: f32,
    ) -> MeasuredAtom<'a> {
        let containing_width = containing_width.max(0.0);
        // Only inline boxes resolve percentage sizing against the containing block. Text,
        // replaced content, and placeholders keep identical measurements across box passes.
        let containing_width_key = if matches!(
            atom,
            InlineAtom::InlineBox { .. } | InlineAtom::BlockBox { .. }
        ) {
            containing_width.to_bits()
        } else {
            0
        };
        let cache_key = (
            atom as *const InlineAtom as usize,
            line_start,
            containing_width_key,
        );
        if let Some(measured) = self.measurement_cache.get(&cache_key) {
            return measured.for_atom(atom);
        }
        let measured = match atom {
            InlineAtom::BlockBox { node, height_basis } => {
                let (minimum, preferred) = self.float_intrinsic_widths(node, containing_width);
                let width = preferred.min(containing_width.max(minimum));
                let height = self.intrinsic_block_height(
                    node,
                    containing_width,
                    *height_basis,
                    Some(UsedInlineSize {
                        outer: width,
                        percentage_basis: containing_width,
                    }),
                );
                MeasuredAtom {
                    atom,
                    text: None,
                    width,
                    height,
                    content_height: height,
                    no_wrap: Node::composed_parent(node).is_some_and(|parent| {
                        self.styles.get(&parent).white_space != WhiteSpace::Normal
                    }),
                    break_before: true,
                }
            }
            InlineAtom::Text {
                text,
                font,
                line_height,
                no_wrap,
                preserve_space,
                ..
            } => {
                let break_before = !preserve_space && text.starts_with(' ');
                let text = if line_start && !preserve_space {
                    text.trim_start_matches(' ')
                } else {
                    text.as_str()
                };
                // Intrinsic sizing and line breaking need advances, not copied raster runs.
                // Request positioned glyphs only when inline_paint emits the final text item.
                let (width, content_height) = self.measurer.measure(text, font);
                MeasuredAtom {
                    atom,
                    text: Some(text),
                    width,
                    height: *line_height,
                    content_height,
                    no_wrap: *no_wrap,
                    break_before,
                }
            }
            InlineAtom::Image { width, height, .. }
            | InlineAtom::Control { width, height, .. }
            | InlineAtom::Placeholder { width, height, .. } => MeasuredAtom {
                atom,
                text: None,
                width: *width,
                height: *height,
                content_height: *height,
                no_wrap: false,
                break_before: false,
            },
            InlineAtom::InlineBox {
                children, style, ..
            } => {
                let metrics = self.measure_inline_box(atom, children, style, containing_width);
                MeasuredAtom {
                    atom,
                    text: None,
                    width: metrics.total_width(),
                    height: metrics.total_height(),
                    content_height: metrics.total_height(),
                    no_wrap: style.white_space == WhiteSpace::NoWrap,
                    break_before: false,
                }
            }
            InlineAtom::Break => unreachable!(),
        };
        self.measurement_cache
            .insert(cache_key, CachedAtomMeasurement::from(&measured));
        measured
    }

    pub(super) fn measure_inline_box(
        &mut self,
        atom: &InlineAtom,
        children: &[InlineAtom],
        style: &ComputedStyle,
        containing_width: f32,
    ) -> InlineBoxMetrics {
        let containing_width = containing_width.max(0.0);
        let cache_key = (
            atom as *const InlineAtom as usize,
            containing_width.to_bits(),
        );
        if let Some(metrics) = self.inline_box_cache.get(&cache_key) {
            return *metrics;
        }

        let margin = style.margin.resolve(containing_width, style.font_size);
        let border = style
            .border_width
            .resolve(containing_width, style.font_size);
        let padding = style.padding.resolve(containing_width, style.font_size);
        let horizontal_insets = border.horizontal() + padding.horizontal();
        let vertical_insets = border.vertical() + padding.vertical();
        let specified_width = resolve_outer_size(
            style.width,
            containing_width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        );
        let child_containing_width = specified_width
            .map(|width| (width - horizontal_insets).max(0.0))
            .unwrap_or(containing_width);
        let mut children_width = 0.0_f32;
        let mut children_height = 0.0_f32;
        for (index, child) in children.iter().enumerate() {
            if matches!(child, InlineAtom::Break) {
                continue;
            }
            let measured = self.measure_atom(child, index == 0, child_containing_width);
            children_width += measured.width;
            children_height = children_height.max(measured.height);
        }

        let mut border_box_width = specified_width.unwrap_or(children_width + horizontal_insets);
        if let Some(minimum) = resolve_outer_size(
            style.min_width,
            containing_width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        ) {
            border_box_width = border_box_width.max(minimum);
        }
        if let Some(maximum) = resolve_outer_size(
            style.max_width,
            containing_width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        ) {
            border_box_width = border_box_width.min(maximum);
        }

        let mut border_box_height = resolve_content_height(
            style.height,
            None,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        )
        .map(|height| height + vertical_insets)
        .unwrap_or(children_height + vertical_insets);
        if let Some(minimum) = resolve_content_height(
            style.min_height,
            None,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        ) {
            border_box_height = border_box_height.max(minimum + vertical_insets);
        }
        if let Some(maximum) = resolve_content_height(
            style.max_height,
            None,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        ) {
            border_box_height = border_box_height.min(maximum + vertical_insets);
        }

        let metrics = InlineBoxMetrics {
            margin,
            border,
            padding,
            border_box_width: border_box_width.max(0.0),
            border_box_height: border_box_height.max(0.0),
            children_width,
        };
        self.inline_box_cache.insert(cache_key, metrics);
        metrics
    }
}
