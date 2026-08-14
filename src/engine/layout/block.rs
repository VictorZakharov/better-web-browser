use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_block(
        &mut self,
        node: &NodeRef,
        containing_x: f32,
        y: f32,
        containing_width: f32,
    ) -> BlockMetrics {
        let style = self.styles.get(node).clone();
        if style.display == Display::None || !style.visibility {
            return BlockMetrics { bottom: y };
        }
        let block_control = input_control_data(node);

        let margins = style.margin.resolve(containing_width, style.font_size);
        let borders = style
            .border_width
            .resolve(containing_width, style.font_size);
        let padding = style.padding.resolve(containing_width, style.font_size);
        let horizontal_insets = padding.horizontal() + borders.horizontal();
        let available_width = (containing_width - margins.horizontal()).max(0.0);
        let mut border_box_width = resolve_outer_size(
            style.width,
            containing_width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        )
        .unwrap_or(available_width);
        if let Some(maximum) = resolve_outer_size(
            style.max_width,
            containing_width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        ) {
            border_box_width = border_box_width.min(maximum);
        }
        if let Some(minimum) = resolve_outer_size(
            style.min_width,
            containing_width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        ) {
            border_box_width = border_box_width.max(minimum);
        }
        border_box_width = border_box_width.max(0.0);

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
                    (containing_x, y, containing_width, self.viewport.height)
                };
            let left = style.left.resolve(positioning_width, style.font_size);
            let right = style.right.resolve(positioning_width, style.font_size);
            if let Some(left) = left {
                x = positioning_x + left;
                if right.is_some()
                    && auto_left
                    && auto_right
                    && border_box_width < positioning_width
                {
                    x = positioning_x + (positioning_width - border_box_width) / 2.0;
                }
            } else if let Some(right) = right {
                x = positioning_x + positioning_width - border_box_width - right;
            }
            if let Some(top) = style.top.resolve(positioning_height, style.font_size) {
                border_y = positioning_y + top;
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

        let content_x = x + borders.left + padding.left;
        let content_y = border_y + borders.top + padding.top;
        let content_width =
            (border_box_width - borders.horizontal() - padding.horizontal()).max(0.0);
        let vertical_insets = borders.vertical() + padding.vertical();
        let specified_height = resolve_content_height(
            style.height,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        );
        let minimum_height = resolve_content_height(
            style.min_height,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        )
        .unwrap_or(0.0);
        let maximum_height = resolve_content_height(
            style.max_height,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        );
        let background_index = if style.background_color.alpha > 0 {
            let index = self.output.items.len();
            self.output.items.push(DisplayItem::SolidRect {
                rect: RectF {
                    x,
                    y: border_y,
                    width: border_box_width,
                    height: 0.0,
                },
                color: self.effective_background_color(node),
                radius: 0.0,
            });
            Some(index)
        } else {
            None
        };
        let background_image_index = style.background_image.as_ref().map(|url| {
            let index = self.output.items.len();
            self.output.items.push(DisplayItem::BackgroundImage {
                clip_rect: RectF {
                    x,
                    y: border_y,
                    width: border_box_width,
                    height: 0.0,
                },
                tile_rect: RectF::default(),
                url: url.clone(),
                repeat_x: style.background_repeat_x,
                repeat_y: style.background_repeat_y,
            });
            index
        });

        let collapsed = style.overflow_hidden && maximum_height.is_some_and(|height| height <= 0.0);
        let content_bottom = if collapsed {
            content_y
        } else if let Some((kind, _)) = block_control.as_ref() {
            content_y + default_control_content_height(node, kind, &style)
        } else {
            match style.display {
                Display::Flex => {
                    self.layout_flex(node, content_x, content_y, content_width, &style)
                }
                Display::Grid => {
                    self.layout_grid(node, content_x, content_y, content_width, &style)
                }
                Display::Table => {
                    self.layout_table(node, content_x, content_y, content_width, &style)
                }
                _ => self.layout_block_children(node, content_x, content_y, content_width, &style),
            }
        };
        let natural_content_height = (content_bottom - content_y).max(0.0);
        let mut content_height = specified_height
            .unwrap_or(natural_content_height)
            .max(minimum_height);
        if let Some(maximum_height) = maximum_height {
            content_height = content_height.min(maximum_height);
        }
        let border_box_height =
            borders.top + padding.top + content_height + padding.bottom + borders.bottom;
        let rect = RectF {
            x,
            y: border_y,
            width: border_box_width,
            height: border_box_height,
        };
        let radius = resolve_border_radius(style.border_radius, rect, style.font_size);
        if let Some(index) = background_index
            && let DisplayItem::SolidRect {
                rect: target,
                radius: target_radius,
                ..
            } = &mut self.output.items[index]
        {
            *target = rect;
            *target_radius = radius;
        }
        if let Some(index) = background_image_index
            && let Some(tile_rect) = self.background_tile_rect(&style, rect)
            && let DisplayItem::BackgroundImage {
                clip_rect,
                tile_rect: target_tile,
                ..
            } = &mut self.output.items[index]
        {
            *clip_rect = rect;
            *target_tile = tile_rect;
        }
        if style.border_color.alpha > 0 && (borders.vertical() > 0.0 || borders.horizontal() > 0.0)
        {
            self.output.items.push(DisplayItem::BorderRect {
                rect,
                widths: [borders.top, borders.right, borders.bottom, borders.left],
                color: style
                    .border_color
                    .composite_over(self.effective_background_color(node)),
                radius,
            });
        }
        if let Some((kind, value)) = block_control {
            let icon = self.control_background_icon(&style, rect.width, rect.height);
            let mut label = input_control_label(node, kind, &value);
            if icon.is_some() && value.is_empty() {
                label.clear();
            }
            self.output
                .items
                .push(DisplayItem::Control(Box::new(ControlSpec {
                    node_id: node_id(node),
                    rect,
                    kind,
                    name: node.attr("name").unwrap_or_default(),
                    value,
                    label,
                    options: Vec::new(),
                    selected_index: 0,
                    placeholder: node
                        .attr("placeholder")
                        .or_else(|| node.attr("title"))
                        .unwrap_or_default(),
                    form_id: nearest_form(node).map(|form| node_id(&form)),
                    background_color: self.effective_background_color(node),
                    text_color: style.color,
                    border_color: style
                        .border_color
                        .composite_over(self.effective_background_color(node)),
                    border_width: [borders.top, borders.right, borders.bottom, borders.left],
                    border_radius: radius,
                    padding: [padding.top, padding.right, padding.bottom, padding.left],
                    font: FontSpec::from_style(&style),
                    icon_url: icon.as_ref().map(|(url, _, _)| url.clone()),
                    icon_width: icon.as_ref().map(|(_, width, _)| *width).unwrap_or(0.0),
                    icon_height: icon.as_ref().map(|(_, _, height)| *height).unwrap_or(0.0),
                })));
        }

        let flow_bottom = border_y + border_box_height + margins.bottom;
        BlockMetrics {
            bottom: if matches!(style.position, Position::Absolute | Position::Fixed) {
                y
            } else {
                flow_bottom
            },
        }
    }

    pub(super) fn layout_block_children(
        &mut self,
        node: &NodeRef,
        x: f32,
        mut y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let positioning_y = y;
        let mut atoms = Vec::new();
        let mut pending_space = false;
        let mut left_float_width = 0.0_f32;
        let mut right_float_width = 0.0_f32;
        let mut float_bottom = y;
        if node.tag_name() == Some("li") {
            atoms.push(InlineAtom::Text {
                text: "• ".into(),
                font: FontSpec::from_style(style),
                color: style.color,
                link: None,
                line_height: style.line_height,
                no_wrap: false,
            });
        }
        for child in node.children.borrow().iter() {
            let child_style = self.styles.get(child);
            if is_block_level(child_style.display)
                && child_style.float != Float::None
                && !matches!(child_style.position, Position::Absolute | Position::Fixed)
            {
                let remaining_width = (width - left_float_width - right_float_width).max(0.0);
                let float_width = self
                    .flex_item_basis(child, child_style, remaining_width)
                    .clamp(0.0, remaining_width);
                let float_x = if child_style.float == Float::Right {
                    x + width - right_float_width - float_width
                } else {
                    x + left_float_width
                };
                let metrics = self.layout_block(child, float_x, y, float_width);
                float_bottom = float_bottom.max(metrics.bottom);
                if child_style.float == Float::Right {
                    right_float_width += float_width;
                } else {
                    left_float_width += float_width;
                }
            } else if is_block_level(child_style.display)
                && !matches!(child_style.position, Position::Absolute | Position::Fixed)
            {
                if !atoms.is_empty() {
                    y = self.layout_inline_atoms(
                        &atoms,
                        x + left_float_width,
                        y,
                        (width - left_float_width - right_float_width).max(0.0),
                        style.text_align,
                        style.line_height,
                    );
                    atoms.clear();
                    pending_space = false;
                }
                y = y.max(float_bottom);
                y = self.layout_block(child, x, y, width).bottom;
            } else if is_block_level(child_style.display) {
                self.layout_block(child, x, positioning_y, width);
            } else {
                self.collect_inline(child, None, &mut atoms, &mut pending_space, true);
            }
        }
        if !atoms.is_empty() {
            y = self.layout_inline_atoms(
                &atoms,
                x + left_float_width,
                y,
                (width - left_float_width - right_float_width).max(0.0),
                style.text_align,
                style.line_height,
            );
        }
        y.max(float_bottom)
    }
}
