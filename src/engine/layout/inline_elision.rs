//! Apply paint elision without feeding shortened text back into source layout.
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_kept_atom(
        &mut self,
        measured: &MeasuredAtom<'_>,
        keep: Option<&AtomKeep>,
        x: f32,
        y: f32,
        line_height: f32,
        containing: f32,
        line_id: u64,
    ) {
        match keep {
            Some(AtomKeep::Whole) => {
                self.paint_atom(measured, x, y, line_height, containing, line_id)
            }
            Some(AtomKeep::TextPrefix { len, width }) => {
                // MeasuredAtom.text can omit LEADING whitespace, never a suffix.
                // Passing a painted prefix to fragment collection would shift DOM
                // offsets and could slice the original UTF-8 string mid-codepoint.
                self.record_atom_fragments(measured, x, y, line_height, line_id);
                self.paint_text_atom(measured, x, y, line_height, Some((*len, *width)));
            }
            Some(AtomKeep::BoxPrefix { keeps, .. }) => {
                let InlineAtom::InlineBox {
                    children,
                    style,
                    node_id,
                } = measured.atom
                else {
                    return;
                };
                self.record_atom_fragments(measured, x, y, line_height, line_id);
                self.paint_inline_box(
                    measured,
                    style,
                    children,
                    *node_id,
                    x,
                    y,
                    line_height,
                    containing,
                    line_id,
                    Some(keeps),
                );
            }
            None => {
                // Hidden ink retains original Range, inline and scroll geometry.
                let emit = self.emit_paint;
                self.emit_paint = false;
                self.paint_atom(measured, x, y, line_height, containing, line_id);
                self.emit_paint = emit;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_truncated_row(
        &mut self,
        line: &[MeasuredAtom<'_>],
        plan: &RowPlan,
        x: f32,
        y: f32,
        available: f32,
        align: TextAlign,
        line_height: f32,
    ) -> f32 {
        let fragments = std::sync::Arc::make_mut(&mut self.output.fragments);
        fragments.next_line += 1;
        let line_id = fragments.next_line;
        let original_width: f32 = line.iter().map(|atom| atom.width).sum();
        let start_x = match align {
            TextAlign::Start => x,
            TextAlign::Center => x + ((available - original_width) / 2.0).max(0.0),
            TextAlign::End => x + (available - original_width).max(0.0),
        };
        let mut cursor_x = start_x;
        let mut keeps = plan.keeps.iter().peekable();
        for (index, measured) in line.iter().enumerate() {
            let keep = if keeps.peek().is_some_and(|(i, _)| *i == index) {
                keeps.next().map(|(_, keep)| keep)
            } else {
                None
            };
            self.paint_kept_atom(measured, keep, cursor_x, y, line_height, available, line_id);
            cursor_x += measured.width;
        }
        let marker = &plan.marker;
        if self.emit_paint && marker.visible {
            let shaped = self.measurer.shape(ELLIPSIS, &marker.font);
            let marker_y = y
                + (line_height - marker.ref_height).max(0.0) / 2.0
                + (marker.ref_height - marker.ref_content_height) / 2.0;
            self.output.items.push(DisplayItem::Text {
                rect: RectF {
                    x: start_x + plan.width - marker.width,
                    y: marker_y,
                    width: marker.width,
                    height: marker.ref_content_height,
                },
                text: ELLIPSIS.to_string(),
                font: marker.font.clone(),
                color: marker.color,
                link: marker.link.clone(),
                node_id: marker.node_id,
                raster_run_id: shaped.raster_run_id,
                glyphs: shaped.glyphs,
            });
        }
        y + line_height
    }
}
