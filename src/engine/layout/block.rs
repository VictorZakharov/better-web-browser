mod children;
mod control;
mod decoration;
pub(super) mod floats;
pub(super) mod margins;
pub(super) mod paint_order;
mod positioned;
mod replaced;
mod sizing;

use super::*;

#[path = "overflow.rs"]
mod overflow;
use sizing::resolve_used_border_box_width;
impl<M: TextMeasurer> LayoutEngine<'_, M> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::engine::layout) fn layout_block_attempt(
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
        let own_context = self.block_establishes_context(node);
        let outer_floats = own_context.then(|| std::mem::take(&mut self.floats));
        let item_start = self.output.items.len();
        let node_start = self.output.node_paint_order.len();
        if self.emit_paint {
            self.positioned_flow_scopes.push(Vec::new());
        }
        if self.emit_paint && !node.is_generated_pseudo() {
            self.output.node_paint_order.push(node_id(node));
        }
        let block_control = input_control_data(node);
        let authored_button = node.tag_name() == Some("button");
        let block_image = self.block_image(node);

        let percentage_basis = used_inline_size
            .map(|size| size.percentage_basis)
            .unwrap_or(containing_width);
        let margin_profile = self.block_margin_profile(node, percentage_basis);
        let mut margins = style.margin.resolve(percentage_basis, style.font_size);
        margins.top = margin_profile.top.size();
        margins.bottom = margin_profile.bottom.size();
        let borders = table::resolved_table_borders(node, &style, percentage_basis);
        let padding = style.padding.resolve(percentage_basis, style.font_size);
        let horizontal_insets = padding.horizontal() + borders.horizontal();
        let available_width = (containing_width - margins.horizontal()).max(0.0);
        let caption_width = table::caption_outer_width(node, percentage_basis, self.styles);
        let normal_automatic_width = block_image.as_ref().map_or(available_width, |image| {
            image.outer_width(node, &style, percentage_basis, horizontal_insets)
        });
        let automatic_width = if authored_button && style.width == Length::Auto {
            self.button_fit_content_width(node, percentage_basis, available_width)
        } else if caption_width > 0.0 {
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
        if style.display.is_table() {
            border_box_width = self.table_used_width(
                node,
                border_box_width,
                horizontal_insets,
                caption_width,
                used_inline_size.is_some(),
            );
        }
        if style.width == Length::Auto
            && !authored_button
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
        let (bar_x, bar_y) = self
            .scroll_gutters
            .get(&node.id())
            .copied()
            .unwrap_or_default();
        let gutter_right = if bar_y {
            self.page.scrollbar_thickness()
        } else {
            0.0
        };
        let gutter_bottom = if bar_x {
            self.page.scrollbar_thickness()
        } else {
            0.0
        };
        let content_width =
            (border_box_width - borders.horizontal() - padding.horizontal() - gutter_right)
                .max(0.0);
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
        let specified_height = specified_height.map(|height| (height - gutter_bottom).max(0.0));
        let minimum_height = (minimum_height - gutter_bottom).max(0.0);
        let maximum_height = maximum_height.map(|height| (height - gutter_bottom).max(0.0));
        let block_image_height = block_image.as_ref().map(|image| {
            image.content_height(node, &style, content_width, percentage_height_basis)
        });
        let decoration = self.begin_block_decoration(
            node,
            &style,
            borders,
            RectF {
                x,
                y: border_y,
                width: border_box_width,
                height: 0.0,
            },
        );
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
                    flex::CrossConstraints {
                        minimum: minimum_height,
                        maximum: maximum_height,
                    },
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
                Display::Table | Display::InlineTable => self.layout_table(
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
                    margin_profile,
                ),
            }
        };
        let content_bottom = if own_context {
            content_bottom.max(self.floats.bottom())
        } else {
            content_bottom
        };
        let natural_content_height = if margin_profile.through {
            0.0
        } else {
            (content_bottom - content_y).max(0.0)
        };
        let used_content_height = if style.display.is_table() {
            natural_content_height
        } else {
            specified_height.unwrap_or(natural_content_height)
        };
        let mut content_height = used_content_height;
        if let Some(maximum_height) = maximum_height {
            content_height = content_height.min(maximum_height);
        }
        content_height = content_height.max(minimum_height);
        content_height = table::cell_content_height(&style, content_height, natural_content_height);
        let offset = table::content_offset(&style, content_height - natural_content_height);
        if offset != 0.0 {
            // Move in-flow content before resolving positioned children against
            // the unshifted containing block.
            self.translate_layout_subtree(
                Some(node),
                in_flow_paint_start,
                self.output.items.len(),
                0.0,
                offset,
            );
        }
        let border_box_height = borders.top
            + padding.top
            + content_height
            + padding.bottom
            + borders.bottom
            + gutter_bottom;
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
            let mut resize =
                ResizeBox::from_content(content_width, content_height, padding, borders);
            resize.border_width += gutter_right;
            resize.border_height += gutter_bottom;
            self.output.resize_boxes.insert(node_id(node), resize);
        }
        let positioning_box = if style.position != Position::Static || !style.transform.is_none() {
            RectF {
                x: x + borders.left,
                y: border_y + borders.top,
                width: (border_box_width - borders.horizontal() - gutter_right).max(0.0),
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
        self.finish_scroll_container(
            node,
            &style,
            RectF {
                x: x + borders.left,
                y: border_y + borders.top,
                width: (border_box_width - borders.horizontal() - gutter_right).max(0.0),
                height: padding.top + content_height + padding.bottom,
            },
            in_flow_paint_start,
        );
        self.finish_overflow_clip(
            overflow_clip,
            RectF {
                x: x + borders.left,
                y: border_y + borders.top,
                width: (border_box_width - borders.horizontal() - gutter_right).max(0.0),
                height: padding.top + content_height + padding.bottom,
            },
        );
        self.paint_scrollbars(node, &style);
        self.finish_block_decoration(&style, rect, decoration);
        if self.emit_paint
            && let Some(image) = block_image
        {
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
        if self.emit_paint && style.position == Position::Sticky {
            self.output.items.insert(
                item_start,
                DisplayItem::NodeBoundary {
                    node_id: node.id(),
                    entering: true,
                },
            );
            self.output.items.push(DisplayItem::NodeBoundary {
                node_id: node.id(),
                entering: false,
            });
        }
        self.wrap_block_paint(node, &style, item_start);
        self.finish_positioned_flow_scope(node.id(), &style, item_start, node_start);

        let flow_bottom = if margin_profile.through {
            y + margin_profile.top.merge(margin_profile.bottom).size()
        } else {
            border_y + border_box_height + margins.bottom
        };
        if let Some(outer) = outer_floats {
            self.floats = outer;
        }
        BlockMetrics {
            bottom: if matches!(style.position, Position::Absolute | Position::Fixed) {
                y
            } else {
                flow_bottom
            },
        }
    }
}
