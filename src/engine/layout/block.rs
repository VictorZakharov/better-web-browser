mod positioned;
mod replaced;

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
        let block_image = self.block_image(node);

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
        .unwrap_or_else(|| {
            block_image.as_ref().map_or(available_width, |image| {
                image.outer_width(node, &style, horizontal_insets)
            })
        });
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

        let (x, border_y) = self.resolve_block_position(
            &style,
            containing_x,
            y,
            containing_width,
            margins,
            border_box_width,
        );

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
        let block_image_height = block_image
            .as_ref()
            .map(|image| image.content_height(node, &style, content_width));
        let background_index = if style.background_color.alpha > 0 && style.mask_image.is_none() {
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
        let mask_image_index = style.mask_image.as_ref().map(|url| {
            let index = self.output.items.len();
            self.output.items.push(DisplayItem::Image {
                rect: RectF {
                    x,
                    y: border_y,
                    width: border_box_width,
                    height: 0.0,
                },
                url: url.clone(),
                alt: String::new(),
                tint: Some(style.background_color),
            });
            index
        });

        let collapsed = style_collapses_overflow(&style, self.viewport);
        let content_bottom = if collapsed {
            content_y
        } else if let Some((kind, _)) = block_control.as_ref() {
            content_y + default_control_content_height(node, kind, &style)
        } else if let Some(height) = block_image_height {
            content_y + height
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
        if let Some(index) = mask_image_index
            && let DisplayItem::Image { rect: target, .. } = &mut self.output.items[index]
        {
            *target = rect;
        }
        if let Some(image) = block_image {
            image.paint(
                node,
                &mut self.output,
                RectF {
                    x: content_x,
                    y: content_y,
                    width: content_width,
                    height: content_height,
                },
            );
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
        if node.tag_name() == Some("li") && style.list_style_type != ListStyleType::None {
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
            if y >= float_bottom {
                left_float_width = 0.0;
                right_float_width = 0.0;
                float_bottom = y;
            }
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
                if y >= float_bottom {
                    left_float_width = 0.0;
                    right_float_width = 0.0;
                }
                let child_width = (width - left_float_width - right_float_width).max(0.0);
                y = self
                    .layout_block(child, x + left_float_width, y, child_width)
                    .bottom;
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
