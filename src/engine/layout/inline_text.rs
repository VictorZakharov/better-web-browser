//! Original source geometry is independent of the elided paint string.
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn paint_text_atom(
        &mut self,
        measured: &MeasuredAtom<'_>,
        x: f32,
        y: f32,
        line_height: f32,
        prefix: Option<(usize, f32)>,
    ) {
        let InlineAtom::Text {
            font,
            color,
            link,
            node_id,
            source_node,
            visible,
            ..
        } = measured.atom
        else {
            return;
        };
        let original = measured.text.unwrap_or_default();
        let text_y = y
            + (line_height - measured.height).max(0.0) / 2.0
            + (measured.height - measured.content_height) / 2.0;
        if !original.is_empty()
            && let Some(id) = source_node
        {
            inline_layout::geometry::include(
                &mut self.output.node_bounds,
                *id,
                RectF {
                    x,
                    y: text_y,
                    width: measured.width,
                    height: measured.content_height,
                },
            );
        }
        let (text, width) = prefix.map_or((original, measured.width), |(len, width)| {
            (&original[..len], width)
        });
        if self.emit_paint && *visible && !text.is_empty() {
            let shaped = self.measurer.shape(text, font);
            self.output.items.push(DisplayItem::Text {
                rect: RectF {
                    x,
                    y: text_y,
                    width,
                    height: measured.content_height,
                },
                text: text.to_string(),
                font: font.clone(),
                color: *color,
                link: link.clone(),
                node_id: *node_id,
                raster_run_id: shaped.raster_run_id,
                glyphs: shaped.glyphs,
            });
            if font.underline {
                let thickness = (font.size / 14.0).clamp(1.0, 3.0);
                self.output.items.push(DisplayItem::SolidRect {
                    rect: RectF {
                        x,
                        y: text_y + measured.content_height - thickness,
                        width,
                        height: thickness,
                    },
                    color: *color,
                    radius: 0.0,
                });
            }
        }
    }
}
