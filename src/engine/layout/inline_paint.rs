use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_line(
        &mut self,
        line: &[MeasuredAtom<'_>],
        x: f32,
        y: f32,
        width: f32,
        align: TextAlign,
        line_width: f32,
        line_height: f32,
        forced_end: bool,
    ) -> f32 {
        let fragments = std::sync::Arc::make_mut(&mut self.output.fragments);
        fragments.next_line += 1;
        let line_id = fragments.next_line;
        let hanging = line
            .last()
            .map(|item| self.hanging_space_width(item.atom))
            .unwrap_or(0.0);
        let hanging = if forced_end && line_width <= width {
            0.0
        } else {
            hanging
        };
        let line_width = (line_width - hanging).max(0.0);
        let mut cursor_x = match align {
            TextAlign::Start => x,
            TextAlign::Center => x + ((width - line_width) / 2.0).max(0.0),
            TextAlign::End => x + (width - line_width).max(0.0),
        };
        for measured in line {
            self.paint_atom(measured, cursor_x, y, line_height, width, line_id);
            cursor_x += measured.width;
        }
        y + line_height
    }

    pub(super) fn paint_atom(
        &mut self,
        measured: &MeasuredAtom<'_>,
        x: f32,
        y: f32,
        line_height: f32,
        containing_width: f32,
        line_id: u64,
    ) {
        self.record_atom_fragments(measured, x, y, line_height, line_id);
        let atom_y = y + (line_height - measured.height).max(0.0) / 2.0;
        match measured.atom {
            InlineAtom::BlockBox { node, height_basis } => {
                self.layout_block(
                    node,
                    x,
                    atom_y,
                    containing_width,
                    *height_basis,
                    Some(UsedInlineSize {
                        outer: measured.width,
                        percentage_basis: containing_width,
                    }),
                );
            }
            InlineAtom::Text { .. } => {
                self.paint_text_atom(measured, x, y, line_height, None);
            }
            InlineAtom::Image {
                resize_box,
                url,
                alt,
                node_id,
                visible,
                inset_x,
                inset_y,
                image_width,
                image_height,
                relative_offset,
                transform,
                transform_font_size,
                opacity,
                tint,
                ..
            } => {
                let item_start = self.output.items.len();
                let mut rect = RectF {
                    x: x + inset_x + relative_offset.0,
                    y: atom_y + inset_y + relative_offset.1,
                    width: *image_width,
                    height: *image_height,
                };
                let (offset_x, offset_y) =
                    transform.resolve(rect.width, rect.height, *transform_font_size);
                rect.x += offset_x;
                rect.y += offset_y;
                self.output.node_bounds.insert(*node_id, rect);
                self.output.resize_boxes.insert(*node_id, *resize_box);
                if self.emit_paint && *visible {
                    if self
                        .styles
                        .node(*node_id)
                        .is_some_and(|node| node.tag_name() == Some("iframe"))
                    {
                        self.output.items.push(DisplayItem::EmbeddedFrame {
                            rect,
                            node_id: *node_id,
                        });
                    } else {
                        self.output.items.push(DisplayItem::Image {
                            rect,
                            url: url.clone(),
                            alt: alt.clone(),
                            tint: *tint,
                        });
                    }
                }
                self.wrap_opacity(item_start, *opacity);
            }
            InlineAtom::Control {
                spec,
                inset_x,
                inset_y,
                control_width,
                control_height,
                opacity,
                ..
            } => {
                let item_start = self.output.items.len();
                let rect = RectF {
                    x: x + inset_x,
                    y: atom_y + inset_y,
                    width: *control_width,
                    height: *control_height,
                };
                self.output.node_bounds.insert(spec.node_id, rect);
                let edges = |e: [f32; 4]| ResolvedEdges {
                    top: e[0],
                    right: e[1],
                    bottom: e[2],
                    left: e[3],
                };
                self.output.resize_boxes.insert(
                    spec.node_id,
                    ResizeBox::from_border(
                        rect.width,
                        rect.height,
                        edges(spec.padding),
                        edges(spec.border_width),
                    ),
                );
                if !self.emit_paint
                    || self
                        .styles
                        .node(spec.node_id)
                        .is_some_and(|node| !self.styles.get(&node).visibility)
                {
                    return;
                }
                let mut spec = spec.as_ref().clone();
                spec.rect = rect;
                if spec.background_color.alpha > 0 {
                    self.output.items.push(DisplayItem::SolidRect {
                        rect: spec.rect,
                        color: spec.background_color,
                        radius: spec.border_radius,
                    });
                }
                if spec.border_colors.iter().any(|c| c.alpha > 0)
                    && spec.border_width.iter().any(|width| *width > 0.0)
                {
                    self.output.items.push(DisplayItem::BorderRect {
                        rect: spec.rect,
                        widths: spec.border_width,
                        colors: spec.border_colors,
                        radius: spec.border_radius,
                    });
                }
                if let Some(url) = spec.icon_url.as_ref() {
                    self.output.items.push(DisplayItem::Image {
                        rect: RectF {
                            x: spec.rect.x + (spec.rect.width - spec.icon_width).max(0.0) / 2.0,
                            y: spec.rect.y + (spec.rect.height - spec.icon_height).max(0.0) / 2.0,
                            width: spec.icon_width.min(spec.rect.width).max(0.0),
                            height: spec.icon_height.min(spec.rect.height).max(0.0),
                        },
                        url: url.clone(),
                        alt: String::new(),
                        tint: None,
                    });
                }
                self.output.items.push(DisplayItem::Control(Box::new(spec)));
                self.wrap_opacity(item_start, *opacity);
            }
            InlineAtom::InlineBox {
                children,
                style,
                node_id,
            } => {
                self.paint_inline_box(
                    measured,
                    style,
                    children,
                    *node_id,
                    x,
                    y,
                    line_height,
                    containing_width,
                    line_id,
                    None,
                );
            }
            InlineAtom::Placeholder {
                node_id,
                resize_box,
                ..
            } => {
                if let Some(node_id) = node_id {
                    self.output.resize_boxes.insert(*node_id, *resize_box);
                    self.output.node_bounds.insert(
                        *node_id,
                        RectF {
                            x,
                            y: atom_y,
                            width: measured.width,
                            height: measured.height,
                        },
                    );
                }
            }
            InlineAtom::Break => {}
        }
    }
}
