use super::inline_layout::LineFormat;
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_line(
        &mut self,
        line: &[MeasuredAtom<'_>],
        x: f32,
        y: f32,
        width: f32,
        format: LineFormat,
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
        // CSS 2.1 §9.4.2: a line holding only empty, undecorated inline boxes is
        // treated as zero-height; its boxes still get positions.
        // A line ended by `<br>` keeps its height, as in browsers' empty editor paragraphs.
        let phantom = !format.broken
            && !line.is_empty()
            && line.iter().all(|measured| is_phantom(measured.atom));
        let mut cursor_x = match format.align {
            TextAlign::Start | TextAlign::Left => x,
            TextAlign::Center => x + ((width - line_width) / 2.0).max(0.0),
            TextAlign::End | TextAlign::Right => x + (width - line_width).max(0.0),
        };
        if let Some(placed) = self.baseline_line(line, format.strut) {
            for measured in line {
                let top = self.baseline_top(measured, format.strut, placed);
                let height = measured.height;
                self.paint_atom(measured, cursor_x, y + top, height, width, line_id);
                cursor_x += measured.width;
            }
            return if phantom { y } else { y + placed.height };
        }
        for measured in line {
            self.paint_atom(measured, cursor_x, y, line_height, width, line_id);
            cursor_x += measured.width;
        }
        if phantom { y } else { y + line_height }
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
                opacity,
                tint,
                ..
            } => {
                let item_start = self.output.items.len();
                let rect = RectF {
                    x: x + inset_x + relative_offset.0,
                    y: atom_y + inset_y + relative_offset.1,
                    width: *image_width,
                    height: *image_height,
                };
                self.output.node_bounds.insert(*node_id, rect);
                self.output.resize_boxes.insert(*node_id, *resize_box);
                if self.emit_paint && *visible {
                    if self
                        .styles
                        .node(*node_id)
                        .is_some_and(|node| node.represents_content_navigable())
                    {
                        self.output.items.push(DisplayItem::EmbeddedFrame {
                            rect,
                            node_id: *node_id,
                        });
                    } else {
                        let style = self
                            .styles
                            .node(*node_id)
                            .map(|node| self.styles.get(&node));
                        let (painted, clip) = style.as_ref().map_or((rect, None), |style| {
                            let (natural_width, natural_height) =
                                if replaced_size_ignores_natural_size(style) {
                                    (*image_width, *image_height)
                                } else {
                                    self.decoded_image(url)
                                        .map(|image| (image.width as f32, image.height as f32))
                                        .unwrap_or((*image_width, *image_height))
                                };
                            object::image_paint_geometry(style, rect, natural_width, natural_height)
                        });
                        self.output.items.push(DisplayItem::Image {
                            rect: painted,
                            clip,
                            url: url.clone(),
                            alt: alt.clone(),
                            tint: *tint,
                        });
                    }
                }
                use crate::engine::affine::Affine2D;
                let matrix = Affine2D::translate(f64::from(rect.x), f64::from(rect.y))
                    .then_local(*transform)
                    .then_local(Affine2D::translate(-f64::from(rect.x), -f64::from(rect.y)));
                if let Some((x, y)) = matrix.translation() {
                    if let Some(node) = self.styles.node(*node_id) {
                        let end = self.output.items.len();
                        self.translate_layout_subtree(Some(&node), item_start, end, x, y);
                    }
                } else {
                    if let Some(node) = self.styles.node(*node_id) {
                        self.transform_geometry(&node, matrix);
                    }
                    self.wrap_transform(item_start, matrix);
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
                        clip: None,
                    });
                }
                let node_id = spec.node_id;
                let content_rect = rect
                    .inset(edges(spec.border_width))
                    .inset(edges(spec.padding));
                self.output.items.push(DisplayItem::Control(Box::new(spec)));
                if let Some(node) = self.styles.node(node_id) {
                    let style = self.styles.get(&node);
                    let reference = if style.transform_box.uses_content() {
                        content_rect
                    } else {
                        rect
                    };
                    self.apply_transform_matrix(
                        node.id(),
                        style.transform_from_reference(reference),
                        item_start,
                    );
                }
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
            InlineAtom::Float { .. } | InlineAtom::Break => {}
        }
    }
}

/// Collapsible white space, or an inline box with no content and no margins, padding or
/// borders. A line of only these is empty (CSS 2.1 §9.4.2; CSS Text 3 §4.1.2 removes
/// collapsible spaces at the start and end of a line).
fn is_phantom(atom: &InlineAtom) -> bool {
    match atom {
        InlineAtom::Text {
            text,
            preserve_space: false,
            ..
        } => text
            .chars()
            .all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')),
        InlineAtom::InlineBox {
            children, style, ..
        } => {
            style.padding == Edges::ZERO
                && style.border_width == Edges::ZERO
                && [&style.margin.left, &style.margin.right]
                    .iter()
                    .all(|margin| matches!(margin, Length::Auto | Length::Px(0.0)))
                && children.iter().all(is_phantom)
        }
        _ => false,
    }
}
