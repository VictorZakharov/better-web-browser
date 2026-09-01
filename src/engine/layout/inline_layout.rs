use super::*;

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
        let mut line = Vec::new();
        let mut line_width = 0.0_f32;
        let mut line_height = 0.0_f32;

        for atom in atoms {
            if matches!(atom, InlineAtom::Break) {
                y = self.paint_line(
                    &line,
                    x,
                    y,
                    width,
                    align,
                    line_width,
                    line_height.max(default_line_height),
                );
                line.clear();
                line_width = 0.0;
                line_height = 0.0;
                continue;
            }
            let measured = self.measure_atom(atom, line.is_empty(), width);
            let should_wrap = !line.is_empty()
                && line_width + measured.width > width
                && measured.break_before
                && !measured.no_wrap;
            if should_wrap {
                y = self.paint_line(
                    &line,
                    x,
                    y,
                    width,
                    align,
                    line_width,
                    line_height.max(default_line_height),
                );
                line.clear();
                line_width = 0.0;
                line_height = 0.0;
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
                x,
                y,
                width,
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
        let containing_width_key = if matches!(atom, InlineAtom::InlineBox { .. }) {
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
            InlineAtom::Text {
                text,
                font,
                line_height,
                no_wrap,
                ..
            } => {
                let break_before = text.chars().next().is_some_and(char::is_whitespace);
                let text = if line_start {
                    text.trim_start()
                } else {
                    text.as_str()
                };
                let shaped = self.measurer.shape(text, font);
                MeasuredAtom {
                    atom,
                    text: Some(text),
                    width: shaped.width,
                    height: line_height.max(shaped.height),
                    no_wrap: *no_wrap,
                    break_before,
                    raster_run_id: shaped.raster_run_id,
                    glyphs: shaped.glyphs,
                }
            }
            InlineAtom::Image { width, height, .. }
            | InlineAtom::Control { width, height, .. }
            | InlineAtom::Placeholder { width, height, .. } => MeasuredAtom {
                atom,
                text: None,
                width: *width,
                height: *height,
                no_wrap: false,
                break_before: false,
                raster_run_id: 0,
                glyphs: Vec::new(),
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
                    no_wrap: style.white_space == WhiteSpace::NoWrap,
                    break_before: false,
                    raster_run_id: 0,
                    glyphs: Vec::new(),
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
