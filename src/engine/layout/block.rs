mod children;
mod control;
mod positioned;
mod replaced;
mod sizing;

use super::*;

#[path = "overflow.rs"]
mod overflow;
use sizing::resolve_used_border_box_width;
impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_block(
        &mut self,
        node: &NodeRef,
        containing_x: f32,
        y: f32,
        containing_width: f32,
        containing_height: Option<f32>,
        used_inline_size: Option<UsedInlineSize>,
    ) -> BlockMetrics {
        self.layout_block_with_content_height(
            node,
            containing_x,
            y,
            containing_width,
            containing_height,
            used_inline_size,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn layout_block_with_content_height(
        &mut self,
        node: &NodeRef,
        containing_x: f32,
        y: f32,
        containing_width: f32,
        containing_height: Option<f32>,
        used_inline_size: Option<UsedInlineSize>,
        used_content_height: Option<f32>,
    ) -> BlockMetrics {
        let style = self.styles.get(node).clone();
        if style.display == Display::None || !style.visibility {
            return BlockMetrics { bottom: y };
        }
        let item_start = self.output.items.len();
        let node_start = self.output.node_paint_order.len();
        self.positioned_flow_scopes.push(Vec::new());
        if !node.is_generated_pseudo() {
            self.output.node_paint_order.push(node_id(node));
        }
        let block_control = input_control_data(node);
        let authored_button = node.tag_name() == Some("button")
            && matches!(
                style.display,
                Display::Flex | Display::InlineFlex | Display::Grid
            );
        let block_image = self.block_image(node);

        let percentage_basis = used_inline_size
            .map(|size| size.percentage_basis)
            .unwrap_or(containing_width);
        let margins = style.margin.resolve(percentage_basis, style.font_size);
        let borders = table::resolved_table_borders(node, &style, percentage_basis);
        let padding = style.padding.resolve(percentage_basis, style.font_size);
        let horizontal_insets = padding.horizontal() + borders.horizontal();
        let available_width = (containing_width - margins.horizontal()).max(0.0);
        let caption_width = if style.display == Display::Table {
            table::caption_outer_width(node, percentage_basis, self.styles)
        } else {
            0.0
        };
        let normal_automatic_width = block_image.as_ref().map_or(available_width, |image| {
            image.outer_width(node, &style, percentage_basis, horizontal_insets)
        });
        let automatic_width = if caption_width > 0.0 {
            caption_width
        } else {
            normal_automatic_width
        };
        let mut border_box_width = resolve_used_border_box_width(
            &style,
            containing_width,
            horizontal_insets,
            margins,
            automatic_width,
            used_inline_size,
        );
        if style.display == Display::Table {
            border_box_width = border_box_width.max(caption_width);
        }
        if style.width == Length::Auto
            && matches!(style.position, Position::Absolute | Position::Fixed)
        {
            let positioning_width = if style.position == Position::Fixed {
                self.viewport.width
            } else {
                containing_width
            };
            if let (Some(left), Some(right)) = (
                style.left.resolve(positioning_width, style.font_size),
                style.right.resolve(positioning_width, style.font_size),
            ) {
                border_box_width =
                    (positioning_width - left - right - margins.horizontal()).max(0.0);
            }
        }
        border_box_width = border_box_width.max(0.0);

        let (x, mut border_y) = self.resolve_block_position(
            &style,
            containing_x,
            y,
            containing_width,
            containing_height,
            margins,
            border_box_width,
        );

        let content_x = x + borders.left + padding.left;
        let mut content_y = border_y + borders.top + padding.top;
        let content_width =
            (border_box_width - borders.horizontal() - padding.horizontal()).max(0.0);
        let vertical_insets = borders.vertical() + padding.vertical();
        let percentage_height_basis = if style.position == Position::Fixed {
            Some(self.viewport.height)
        } else {
            containing_height
        };
        let (specified_height, minimum_height, maximum_height) = sizing::resolve_height_constraints(
            &style,
            used_content_height,
            percentage_height_basis,
            self.viewport,
            vertical_insets,
            margins,
        );
        let block_image_height = block_image.as_ref().map(|image| {
            image.content_height(node, &style, content_width, percentage_height_basis)
        });
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
        let overflow_clip = self.begin_overflow_clip(&style);
        // Negative positioned levels paint after this background and before in-flow descendants.
        let in_flow_paint_start = self.output.items.len();
        let in_flow_node_start = self.output.node_paint_order.len();

        let collapsed = style_collapses_overflow(&style, self.viewport);
        let content_bottom = if collapsed {
            content_y
        } else if let Some((kind, _)) = block_control.as_ref().filter(|_| !authored_button) {
            content_y + default_control_content_height(node, kind, &style)
        } else if let Some(height) = block_image_height {
            content_y + height
        } else {
            match style.display {
                Display::Flex | Display::InlineFlex => self.layout_flex(
                    node,
                    content_x,
                    content_y,
                    content_width,
                    specified_height,
                    &style,
                ),
                Display::Grid => self.layout_grid(
                    node,
                    content_x,
                    content_y,
                    content_width,
                    specified_height,
                    &style,
                ),
                Display::Table => self.layout_table(
                    node,
                    content_x,
                    content_y,
                    content_width,
                    specified_height,
                    &style,
                ),
                _ => self.layout_block_children(
                    node,
                    content_x,
                    content_y,
                    content_width,
                    specified_height,
                    &style,
                ),
            }
        };
        let natural_content_height = (content_bottom - content_y).max(0.0);
        let used_content_height = if style.display == Display::Table {
            natural_content_height
        } else {
            specified_height.unwrap_or(natural_content_height)
        };
        let mut content_height = used_content_height.max(minimum_height);
        if let Some(maximum_height) = maximum_height {
            content_height = content_height.min(maximum_height);
        }
        let border_box_height =
            borders.top + padding.top + content_height + padding.bottom + borders.bottom;
        let bottom_shift = positioned::bottom_alignment_shift(
            &style,
            percentage_height_basis.unwrap_or(self.viewport.height),
            border_box_height,
            margins.bottom,
        );
        if bottom_shift != 0.0 {
            self.translate_layout_subtree(
                Some(node),
                item_start,
                self.output.items.len(),
                0.0,
                bottom_shift,
            );
            border_y += bottom_shift;
            content_y += bottom_shift;
        }
        let rect = RectF {
            x,
            y: border_y,
            width: border_box_width,
            height: border_box_height,
        };
        if !node.is_generated_pseudo() {
            self.output.node_bounds.insert(node_id(node), rect);
        }
        let positioning_box = if style.position != Position::Static || !style.transform.is_none() {
            RectF {
                x: x + borders.left,
                y: border_y + borders.top,
                width: (border_box_width - borders.horizontal()).max(0.0),
                height: padding.top + content_height + padding.bottom,
            }
        } else {
            // A static box does not establish an absolute-position containing block. Preserve
            // the context selected by its nearest positioned ancestor (or the initial block).
            RectF {
                x: containing_x,
                y,
                width: containing_width,
                height: containing_height.unwrap_or(self.viewport.height),
            }
        };
        self.layout_positioned_children(
            node,
            positioning_box,
            in_flow_paint_start,
            in_flow_node_start,
        );
        self.finish_overflow_clip(
            overflow_clip,
            RectF {
                x: x + borders.left,
                y: border_y + borders.top,
                width: (border_box_width - borders.horizontal()).max(0.0),
                height: padding.top + content_height + padding.bottom,
            },
        );
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
        self.project_control(
            node,
            block_control,
            &style,
            rect,
            borders,
            padding,
            authored_button,
        );
        if node.is_generated_pseudo() {
            self.apply_generated_transform(&style, rect, item_start);
        } else {
            self.apply_transform(node.id(), &style, rect, item_start);
        }
        self.wrap_opacity(item_start, style.opacity);
        self.finish_positioned_flow_scope(node.id(), &style, item_start, node_start);

        let flow_bottom = border_y + border_box_height + margins.bottom;
        BlockMetrics {
            bottom: if matches!(style.position, Position::Absolute | Position::Fixed) {
                y
            } else {
                flow_bottom
            },
        }
    }
}
