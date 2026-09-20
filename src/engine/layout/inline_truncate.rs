//! Painting for truncated line boxes: kept atoms plus the synthetic marker.
//!
//! Kept atoms paint through the ordinary atom path, so fragments, node bounds
//! and paint agree exactly as they do for untruncated lines. The marker itself
//! is generated text: it carries paint (with the truncated link target) but no
//! source clusters, keeping DOM offsets intact. The clamp-aware line flush
//! lives here as well so the core line breaker in `inline_layout` stays small.

use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// Paints one accumulated line, applying the clamp budget and the
    /// single-line overflow marker. Suppressed lines return `y` unchanged so
    /// following siblings settle on the clamped box size.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn flush_line(
        &mut self,
        line: &mut Vec<MeasuredAtom<'_>>,
        line_x: f32,
        y: f32,
        available: f32,
        align: TextAlign,
        line_width: f32,
        line_height: f32,
        forced_end: bool,
        policy: &TruncationPolicy,
        clamp: &mut Option<ClampState>,
        more: bool,
    ) -> f32 {
        if line.is_empty() {
            // Blank lines from interior breaks keep their spacing, but a spent
            // clamp budget suppresses them like any later line.
            if let Some(state) = clamp.as_mut() {
                if state.finished || policy.max_lines.is_some_and(|max| state.lines_used >= max) {
                    state.finished = true;
                    return y;
                }
                let y = self.paint_line(
                    line,
                    line_x,
                    y,
                    available,
                    align,
                    line_width,
                    line_height,
                    forced_end,
                );
                state.lines_used += 1;
                return y;
            }
            return self.paint_line(
                line,
                line_x,
                y,
                available,
                align,
                line_width,
                line_height,
                forced_end,
            );
        }
        if let Some(state) = clamp.as_mut() {
            let max = policy.max_lines.unwrap_or(u32::MAX);
            if state.finished || state.lines_used >= max {
                state.finished = true;
                line.clear();
                return y;
            }
            if state.lines_used + 1 >= max && more {
                // Final visible line with content following: force the marker.
                if let Some(plan) = self.plan_truncated_row(line, available, available, policy) {
                    let y = self.paint_truncated_row(
                        line,
                        &plan,
                        line_x,
                        y,
                        available,
                        align,
                        line_height,
                    );
                    state.lines_used += 1;
                    state.finished = true;
                    line.clear();
                    return y;
                }
                // The marker cannot fit: paint untruncated (as Chrome does for
                // narrow boxes) while still spending the budget.
                let y = self.paint_line(
                    line,
                    line_x,
                    y,
                    available,
                    align,
                    line_width,
                    line_height,
                    forced_end,
                );
                state.lines_used += 1;
                state.finished = true;
                line.clear();
                return y;
            }
            let y = if policy.single_line_ellipsis && line_width > available {
                match self.plan_truncated_row(line, available, available, policy) {
                    Some(plan) => self.paint_truncated_row(
                        line,
                        &plan,
                        line_x,
                        y,
                        available,
                        align,
                        line_height,
                    ),
                    None => self.paint_line(
                        line,
                        line_x,
                        y,
                        available,
                        align,
                        line_width,
                        line_height,
                        forced_end,
                    ),
                }
            } else {
                self.paint_line(
                    line,
                    line_x,
                    y,
                    available,
                    align,
                    line_width,
                    line_height,
                    forced_end,
                )
            };
            state.lines_used += 1;
            line.clear();
            return y;
        }
        if policy.single_line_ellipsis && line_width > available {
            let y = match self.plan_truncated_row(line, available, available, policy) {
                Some(plan) => {
                    self.paint_truncated_row(line, &plan, line_x, y, available, align, line_height)
                }
                None => self.paint_line(
                    line,
                    line_x,
                    y,
                    available,
                    align,
                    line_width,
                    line_height,
                    forced_end,
                ),
            };
            line.clear();
            return y;
        }
        let y = self.paint_line(
            line,
            line_x,
            y,
            available,
            align,
            line_width,
            line_height,
            forced_end,
        );
        line.clear();
        y
    }

    /// Paints one inline box. With `truncation`, only kept children paint and
    /// the border box shrinks to them; the line-level ellipsis marker itself is
    /// painted by the truncated-row walk, never here.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_inline_box(
        &mut self,
        measured: &MeasuredAtom<'_>,
        style: &ComputedStyle,
        children: &[InlineAtom],
        node_id: Option<NodeId>,
        x: f32,
        y: f32,
        line_height: f32,
        containing_width: f32,
        line_id: u64,
        truncation: Option<BoxPaint<'_>>,
    ) {
        let atom_y = y + (line_height - measured.height).max(0.0) / 2.0;
        let item_start = self.output.items.len();
        let metrics = self.measure_inline_box(measured.atom, children, style, containing_width);
        let border_box_width = truncation
            .as_ref()
            .map_or(metrics.border_box_width, |paint| paint.border_box_width);
        let kept_children_width = truncation
            .as_ref()
            .map_or(metrics.children_width, |paint| paint.children_width);
        let border_x = x + metrics.margin.left;
        let border_y = if metrics.border_box_height == 0.0 && children.is_empty() {
            y + metrics.margin.top
        } else {
            atom_y + metrics.margin.top
        };
        let border_rect = RectF {
            x: border_x,
            y: border_y,
            width: border_box_width,
            height: metrics.border_box_height,
        };
        if let Some(node_id) = node_id {
            self.output.node_bounds.insert(node_id, border_rect);
            // Non-replaced inline elements have an empty ResizeObserver content rect.
            if style.display != Display::Inline {
                self.output.resize_boxes.insert(
                    node_id,
                    ResizeBox::from_border(
                        border_rect.width,
                        border_rect.height,
                        metrics.padding,
                        metrics.border,
                    ),
                );
            }
        }
        let radius = resolve_border_radius(style.border_radius, border_rect, style.font_size);
        if self.emit_paint
            && style.visibility
            && style.background_color.alpha > 0
            && style.mask_image.is_none()
        {
            self.output.items.push(DisplayItem::SolidRect {
                rect: border_rect,
                color: style
                    .background_color
                    .composite_over(self.output.background),
                radius,
            });
        }
        if self.emit_paint
            && style.visibility
            && let Some(tile_rect) = self.background_tile_rect(style, border_rect)
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
        if self.emit_paint
            && style.visibility
            && let Some(url) = style.mask_image.as_ref()
        {
            self.output.items.push(DisplayItem::Image {
                rect: border_rect,
                url: url.clone(),
                alt: String::new(),
                tint: Some(style.background_color),
            });
        }
        if self.emit_paint
            && style.visibility
            && style.resolved_border_colors().iter().any(|c| c.alpha > 0)
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
                colors: style.painted_border_colors(
                    style
                        .background_color
                        .composite_over(self.output.background),
                ),
                radius,
            });
        }
        let content_x = border_x + metrics.border.left + metrics.padding.left;
        let content_y = border_y + metrics.border.top + metrics.padding.top;
        let content_width =
            (border_box_width - metrics.border.horizontal() - metrics.padding.horizontal())
                .max(0.0);
        let content_height =
            (metrics.border_box_height - metrics.border.vertical() - metrics.padding.vertical())
                .max(0.0);
        let mut child_x = match style.text_align {
            TextAlign::Start => content_x,
            TextAlign::Center => content_x + ((content_width - kept_children_width) / 2.0).max(0.0),
            TextAlign::End => content_x + (content_width - kept_children_width).max(0.0),
        };
        // Kept indices address child positions, translated from row positions
        // by the planner. Absent indices are dropped truncated content.
        let keep_for: Vec<Option<&AtomKeep>> = children
            .iter()
            .enumerate()
            .map(|(index, _)| {
                truncation.as_ref().and_then(|paint| {
                    paint
                        .keeps
                        .iter()
                        .find(|(keep_index, _)| *keep_index == index)
                        .map(|(_, keep)| keep)
                })
            })
            .collect();
        for (index, child) in children.iter().enumerate() {
            if matches!(child, InlineAtom::Break) {
                continue;
            }
            match keep_for[index] {
                None if truncation.is_some() => continue,
                None | Some(AtomKeep::Whole) => {
                    let painted = self.measure_atom(child, index == 0, content_width);
                    self.paint_atom(
                        &painted,
                        child_x,
                        content_y,
                        content_height.max(painted.height),
                        content_width,
                        line_id,
                    );
                    child_x += painted.width;
                }
                Some(AtomKeep::TextPrefix { len, width }) => {
                    let painted = self.measure_atom(child, index == 0, content_width);
                    let text = painted.text.unwrap_or_default();
                    let prefix = text.get(..*len).unwrap_or_default();
                    let shortened = MeasuredAtom {
                        atom: child,
                        text: Some(prefix),
                        width: *width,
                        height: painted.height,
                        content_height: painted.content_height,
                        no_wrap: painted.no_wrap,
                        break_before: painted.break_before,
                    };
                    self.paint_atom(
                        &shortened,
                        child_x,
                        content_y,
                        content_height.max(shortened.height),
                        content_width,
                        line_id,
                    );
                    child_x += *width;
                }
                Some(AtomKeep::BoxPrefix {
                    keeps,
                    width,
                    children_width,
                }) => {
                    let InlineAtom::InlineBox {
                        children: nested_children,
                        style: nested_style,
                        node_id: nested_id,
                    } = child
                    else {
                        continue;
                    };
                    let painted = self.measure_atom(child, index == 0, content_width);
                    self.paint_inline_box(
                        &painted,
                        nested_style,
                        nested_children,
                        *nested_id,
                        child_x,
                        content_y,
                        content_height.max(painted.height),
                        content_width,
                        line_id,
                        Some(BoxPaint {
                            keeps,
                            border_box_width: *width,
                            children_width: *children_width,
                        }),
                    );
                    child_x += *width;
                }
            }
        }
        if let Some(node_id) = node_id {
            self.apply_transform(node_id, style, border_rect, item_start);
        } else {
            self.apply_generated_transform(style, border_rect, item_start);
        }
        self.wrap_opacity(item_start, style.opacity);
    }
}
