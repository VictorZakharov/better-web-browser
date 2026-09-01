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
    ) -> f32 {
        let mut cursor_x = match align {
            TextAlign::Start => x,
            TextAlign::Center => x + ((width - line_width) / 2.0).max(0.0),
            TextAlign::End => x + (width - line_width).max(0.0),
        };
        for measured in line {
            self.paint_atom(measured, cursor_x, y, line_height, width);
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
    ) {
        let atom_y = y + (line_height - measured.height).max(0.0) / 2.0;
        match measured.atom {
            InlineAtom::Text {
                font,
                color,
                link,
                node_id,
                ..
            } => {
                let text = measured.text.unwrap_or_default();
                if !text.is_empty() {
                    self.output.items.push(DisplayItem::Text {
                        rect: RectF {
                            x,
                            y: atom_y,
                            width: measured.width,
                            height: measured.height,
                        },
                        text: text.to_string(),
                        font: font.clone(),
                        color: *color,
                        link: link.clone(),
                        node_id: *node_id,
                        raster_run_id: measured.raster_run_id,
                        glyphs: measured.glyphs.clone(),
                    });
                    if font.underline {
                        let thickness = (font.size / 14.0).clamp(1.0, 3.0);
                        self.output.items.push(DisplayItem::SolidRect {
                            rect: RectF {
                                x,
                                y: atom_y + measured.height - thickness,
                                width: measured.width,
                                height: thickness,
                            },
                            color: *color,
                            radius: 0.0,
                        });
                    }
                }
            }
            InlineAtom::Image {
                url,
                alt,
                node_id,
                visible,
                inset_x,
                inset_y,
                image_width,
                image_height,
                transform,
                transform_font_size,
                opacity,
                tint,
                ..
            } => {
                let item_start = self.output.items.len();
                let mut rect = RectF {
                    x: x + inset_x,
                    y: atom_y + inset_y,
                    width: *image_width,
                    height: *image_height,
                };
                let (offset_x, offset_y) =
                    transform.resolve(rect.width, rect.height, *transform_font_size);
                rect.x += offset_x;
                rect.y += offset_y;
                self.output.node_bounds.insert(*node_id, rect);
                if *visible {
                    self.output.items.push(DisplayItem::Image {
                        rect,
                        url: url.clone(),
                        alt: alt.clone(),
                        tint: *tint,
                    });
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
                let mut spec = spec.as_ref().clone();
                spec.rect = RectF {
                    x: x + inset_x,
                    y: atom_y + inset_y,
                    width: *control_width,
                    height: *control_height,
                };
                self.output.node_bounds.insert(spec.node_id, spec.rect);
                if spec.background_color.alpha > 0 {
                    self.output.items.push(DisplayItem::SolidRect {
                        rect: spec.rect,
                        color: spec.background_color,
                        radius: spec.border_radius,
                    });
                }
                if spec.border_color.alpha > 0 && spec.border_width.iter().any(|width| *width > 0.0)
                {
                    self.output.items.push(DisplayItem::BorderRect {
                        rect: spec.rect,
                        widths: spec.border_width,
                        color: spec.border_color,
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
                let item_start = self.output.items.len();
                let metrics =
                    self.measure_inline_box(measured.atom, children, style, containing_width);
                let border_x = x + metrics.margin.left;
                let border_y = if metrics.border_box_height == 0.0 && children.is_empty() {
                    y + metrics.margin.top
                } else {
                    atom_y + metrics.margin.top
                };
                let border_rect = RectF {
                    x: border_x,
                    y: border_y,
                    width: metrics.border_box_width,
                    height: metrics.border_box_height,
                };
                if let Some(node_id) = node_id {
                    self.output.node_bounds.insert(*node_id, border_rect);
                }
                let radius =
                    resolve_border_radius(style.border_radius, border_rect, style.font_size);
                if style.background_color.alpha > 0 && style.mask_image.is_none() {
                    self.output.items.push(DisplayItem::SolidRect {
                        rect: border_rect,
                        color: style
                            .background_color
                            .composite_over(self.output.background),
                        radius,
                    });
                }
                if let Some(tile_rect) = self.background_tile_rect(style, border_rect)
                    && let Some(url) = style.background_image.as_ref()
                {
                    self.output.items.push(DisplayItem::BackgroundImage {
                        clip_rect: border_rect,
                        tile_rect,
                        url: url.clone(),
                        repeat_x: style.background_repeat_x,
                        repeat_y: style.background_repeat_y,
                    });
                }
                if let Some(url) = style.mask_image.as_ref() {
                    self.output.items.push(DisplayItem::Image {
                        rect: border_rect,
                        url: url.clone(),
                        alt: String::new(),
                        tint: Some(style.background_color),
                    });
                }
                if style.border_color.alpha > 0
                    && (metrics.border.horizontal() > 0.0 || metrics.border.vertical() > 0.0)
                {
                    self.output.items.push(DisplayItem::BorderRect {
                        rect: border_rect,
                        widths: [
                            metrics.border.top,
                            metrics.border.right,
                            metrics.border.bottom,
                            metrics.border.left,
                        ],
                        color: style.border_color.composite_over(
                            style
                                .background_color
                                .composite_over(self.output.background),
                        ),
                        radius,
                    });
                }
                let content_x = border_x + metrics.border.left + metrics.padding.left;
                let content_y = border_y + metrics.border.top + metrics.padding.top;
                let content_width = (metrics.border_box_width
                    - metrics.border.horizontal()
                    - metrics.padding.horizontal())
                .max(0.0);
                let content_height = (metrics.border_box_height
                    - metrics.border.vertical()
                    - metrics.padding.vertical())
                .max(0.0);
                let mut child_x = match style.text_align {
                    TextAlign::Start => content_x,
                    TextAlign::Center => {
                        content_x + ((content_width - metrics.children_width) / 2.0).max(0.0)
                    }
                    TextAlign::End => content_x + (content_width - metrics.children_width).max(0.0),
                };
                for (index, child) in children.iter().enumerate() {
                    if matches!(child, InlineAtom::Break) {
                        continue;
                    }
                    let child = self.measure_atom(child, index == 0, content_width);
                    self.paint_atom(
                        &child,
                        child_x,
                        content_y,
                        content_height.max(child.height),
                        content_width,
                    );
                    child_x += child.width;
                }
                if let Some(node_id) = node_id {
                    self.apply_transform(*node_id, style, border_rect, item_start);
                } else {
                    self.apply_generated_transform(style, border_rect, item_start);
                }
                self.wrap_opacity(item_start, style.opacity);
            }
            InlineAtom::Placeholder { node_id, .. } => {
                if let Some(node_id) = node_id {
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
