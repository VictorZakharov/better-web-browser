use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_flex(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let composed_children = Node::composed_children(node);
        let has_direct_text = composed_children.iter().any(
            |child| matches!(&child.data, NodeData::Text(text) if !text.borrow().trim().is_empty()),
        );
        let element_children = composed_children
            .iter()
            .filter(|child| {
                child.element().is_some()
                    && self.styles.get(child).display != Display::None
                    && self.styles.get(child).visibility
                    && !style_collapses_overflow(self.styles.get(child), self.viewport)
            })
            .cloned()
            .collect::<Vec<_>>();
        if element_children.is_empty() || has_direct_text {
            return self.layout_flattened_flex_content(node, x, y, width, style);
        }

        for child in element_children.iter().filter(|child| {
            matches!(
                self.styles.get(child).position,
                Position::Absolute | Position::Fixed
            )
        }) {
            self.layout_block(child, x, y, width);
        }
        let items = element_children
            .into_iter()
            .filter(|child| {
                !matches!(
                    self.styles.get(child).position,
                    Position::Absolute | Position::Fixed
                )
            })
            .map(|child| {
                let child_style = self.styles.get(&child).clone();
                FlexItem {
                    basis: self.flex_item_basis(&child, &child_style, width),
                    grow: child_style.flex_grow,
                    shrink: child_style.flex_shrink,
                    margin_start_auto: child_style.margin.left == Length::Auto,
                    margin_end_auto: child_style.margin.right == Length::Auto,
                    node: child,
                }
            })
            .collect::<Vec<_>>();
        if items.is_empty() {
            return y;
        }

        match style.flex_direction {
            FlexDirection::Column => self.layout_flex_column(&items, x, y, width, style),
            FlexDirection::Row => self.layout_flex_rows(&items, x, y, width, style),
        }
    }

    pub(super) fn layout_flattened_flex_content(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let mut atoms = Vec::new();
        let mut pending_space = false;
        for child in Node::composed_children(node).iter() {
            self.collect_inline(child, None, &mut atoms, &mut pending_space, false);
        }
        let alignment = if style.justify_content_end || style.float == Float::Right {
            TextAlign::End
        } else {
            style.text_align
        };
        self.layout_inline_atoms(&atoms, x, y, width, alignment, style.line_height)
    }

    pub(super) fn flex_item_basis(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        available_width: f32,
    ) -> f32 {
        let margin = style.margin.resolve(available_width, style.font_size);
        let border = style.border_width.resolve(available_width, style.font_size);
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
        } else {
            let mut atoms = Vec::new();
            let mut pending_space = false;
            for child in Node::composed_children(node).iter() {
                self.collect_inline(child, None, &mut atoms, &mut pending_space, false);
            }
            self.begin_inline_measurement_context();
            let mut intrinsic_width = 0.0_f32;
            let mut current_line = 0.0_f32;
            let mut line_start = true;
            for atom in &atoms {
                if matches!(atom, InlineAtom::Break) {
                    intrinsic_width = intrinsic_width.max(current_line);
                    current_line = 0.0;
                    line_start = true;
                } else {
                    current_line += self.measure_atom(atom, line_start, available_width).width;
                    line_start = false;
                }
            }
            intrinsic_width.max(current_line)
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

    pub(super) fn layout_flex_column(
        &mut self,
        items: &[FlexItem],
        x: f32,
        mut y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let gap = style
            .grid_row_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        for (index, item) in items.iter().enumerate() {
            y = self.layout_flex_item(&item.node, x, y, width).bottom;
            if index + 1 < items.len() {
                y += gap;
            }
        }
        y
    }

    pub(super) fn layout_flex_rows(
        &mut self,
        items: &[FlexItem],
        x: f32,
        y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let gap = style
            .grid_column_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let mut lines = Vec::<Vec<FlexItem>>::new();
        let mut current = Vec::new();
        let mut current_width = 0.0_f32;
        for item in items {
            let next_width = if current.is_empty() {
                item.basis
            } else {
                current_width + gap + item.basis
            };
            if style.flex_wrap && !current.is_empty() && next_width > width {
                lines.push(std::mem::take(&mut current));
                current_width = 0.0;
            }
            if !current.is_empty() {
                current_width += gap;
            }
            current_width += item.basis;
            current.push(item.clone());
        }
        if !current.is_empty() {
            lines.push(current);
        }

        let row_gap = style
            .grid_row_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let mut cursor_y = y;
        let line_count = lines.len();
        for (index, line) in lines.iter().enumerate() {
            cursor_y = self.layout_flex_row_line(line, x, cursor_y, width, gap, style);
            if index + 1 < line_count {
                cursor_y += row_gap;
            }
        }
        cursor_y
    }

    pub(super) fn layout_flex_row_line(
        &mut self,
        items: &[FlexItem],
        x: f32,
        y: f32,
        width: f32,
        base_gap: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let gap_width = base_gap * items.len().saturating_sub(1) as f32;
        let mut sizes = items.iter().map(|item| item.basis).collect::<Vec<_>>();
        let basis_sum = sizes.iter().sum::<f32>();
        let free = width - gap_width - basis_sum;
        if free > 0.0 {
            let total_grow = items.iter().map(|item| item.grow).sum::<f32>();
            if total_grow > 0.0 {
                for (size, item) in sizes.iter_mut().zip(items) {
                    *size += free * item.grow / total_grow;
                }
            }
        } else if free < 0.0 {
            let total_shrink = items
                .iter()
                .map(|item| item.shrink * item.basis)
                .sum::<f32>();
            if total_shrink > 0.0 {
                for (size, item) in sizes.iter_mut().zip(items) {
                    let shrink = -free * item.shrink * item.basis / total_shrink;
                    *size = (*size - shrink).max(1.0);
                }
            }
        }

        let unused = (width - gap_width - sizes.iter().sum::<f32>()).max(0.0);
        let automatic_margin_count = items
            .iter()
            .map(|item| item.margin_start_auto as usize + item.margin_end_auto as usize)
            .sum::<usize>();
        let automatic_margin = if automatic_margin_count > 0 {
            unused / automatic_margin_count as f32
        } else {
            0.0
        };
        let justify_space = if automatic_margin_count > 0 {
            0.0
        } else {
            unused
        };
        let (offset, extra_gap) = match style.justify_content {
            JustifyContent::End => (justify_space, 0.0),
            JustifyContent::Center => (justify_space / 2.0, 0.0),
            JustifyContent::SpaceBetween if items.len() > 1 => {
                (0.0, justify_space / (items.len() - 1) as f32)
            }
            JustifyContent::SpaceAround => {
                let share = justify_space / items.len() as f32;
                (share / 2.0, share)
            }
            JustifyContent::SpaceEvenly => {
                let share = justify_space / (items.len() + 1) as f32;
                (share, share)
            }
            _ => (0.0, 0.0),
        };

        let mut cursor_x = x + offset;
        let mut painted = Vec::with_capacity(items.len());
        let mut row_height = 0.0_f32;
        for (index, (item, item_width)) in items.iter().zip(sizes).enumerate() {
            if item.margin_start_auto {
                cursor_x += automatic_margin;
            }
            let output_start = self.output.items.len();
            let metrics = self.layout_flex_item(&item.node, cursor_x, y, item_width.max(1.0));
            let output_end = self.output.items.len();
            let item_height = (metrics.bottom - y).max(0.0);
            row_height = row_height.max(item_height);
            painted.push((output_start, output_end, item_height));
            cursor_x += item_width;
            if item.margin_end_auto {
                cursor_x += automatic_margin;
            }
            if index + 1 < items.len() {
                cursor_x += base_gap + extra_gap;
            }
        }

        let cross_size = resolve_height_value(style.height, self.viewport, style.font_size)
            .unwrap_or(row_height)
            .max(row_height);
        for (start, end, item_height) in painted {
            let offset_y = match style.align_items {
                AlignItems::Center => (cross_size - item_height) / 2.0,
                AlignItems::End => cross_size - item_height,
                AlignItems::Stretch | AlignItems::Start => 0.0,
            };
            if offset_y > 0.0 {
                translate_display_items(&mut self.output.items[start..end], 0.0, offset_y);
            }
        }
        y + cross_size
    }

    pub(super) fn layout_flex_item(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
    ) -> BlockMetrics {
        let tag = node.tag_name().unwrap_or_default();
        if !matches!(
            tag,
            "img" | "image" | "input" | "textarea" | "button" | "svg"
        ) {
            return self.layout_block(node, x, y, width);
        }

        let mut style = self.styles.get(node).clone();
        let margin = style.margin.resolve(width, style.font_size);
        let border = style.border_width.resolve(width, style.font_size);
        let padding = style.padding.resolve(width, style.font_size);
        let border_box_width = (width - margin.horizontal()).max(1.0);
        style.width = Length::Px(if style.box_sizing == BoxSizing::BorderBox {
            border_box_width
        } else {
            (border_box_width - border.horizontal() - padding.horizontal()).max(1.0)
        });

        let mut atoms = Vec::new();
        match tag {
            "img" | "image" => self.collect_image(node, &style, None, &mut atoms),
            "input" | "textarea" => self.collect_input(node, &style, &mut atoms),
            "button" => self.collect_button(node, &style, &mut atoms),
            "svg" => self.collect_svg(node, &style, &mut atoms),
            _ => {}
        }
        let bottom =
            self.layout_inline_atoms(&atoms, x, y, width, style.text_align, style.line_height);
        BlockMetrics { bottom }
    }
}
