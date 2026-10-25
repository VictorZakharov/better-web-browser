//! CSS Text Decoration 3 §4 `text-shadow`: offset copies of a text run painted beneath it.
//!
//! The first shadow is on top, so layers are emitted back to front. Blur radii are not
//! rasterized yet; each layer paints as a sharp copy at its offset. Shadow copies carry no
//! link, hit-test node or `color_owner`, so neither hit testing nor a color-only restyle
//! treats them as the text itself (a `currentColor` shadow opts out of that fast path).

use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn paint_text_shadows(
        &mut self,
        atom: &InlineAtom,
        text: &str,
        rect: RectF,
        shaped: &ShapedText,
    ) {
        let InlineAtom::Text {
            font,
            color,
            color_owner,
            ..
        } = atom
        else {
            return;
        };
        let Some(owner) = color_owner.and_then(|id| self.styles.node(id)) else {
            return;
        };
        let style = self.styles.get(&owner);
        if style.text_shadows.is_empty() {
            return;
        }
        let text: std::sync::Arc<str> = text.into();
        for shadow in style.text_shadows.iter().rev() {
            let shadow_color = shadow.color.unwrap_or(*color);
            if shadow_color.alpha == 0 {
                continue;
            }
            let offset = |index: usize| {
                shadow.lengths[index]
                    .resolve(0.0, style.font_size)
                    .unwrap_or(0.0)
            };
            self.output.items.push(DisplayItem::Text {
                rect: RectF {
                    x: rect.x + offset(0),
                    y: rect.y + offset(1),
                    ..rect
                },
                text: text.clone(),
                font: font.clone(),
                color: shadow_color,
                link: None,
                node_id: None,
                color_owner: None,
                raster_run_id: shaped.raster_run_id,
                glyphs: shaped.glyphs.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

    #[test]
    fn text_shadows_paint_offset_copies_beneath_the_text_first_shadow_on_top() {
        // CSS Text Decoration 3 §4: shadows are inherited, offset by their lengths, use
        // currentColor when no color is given, and stack with the first one on top.
        let page = Page::parse(
            "<style>body{margin:0;color:black}h1{text-shadow:silver 3px 3px,0 0.5em}</style>\
             <h1><span>A</span></h1>",
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let runs: Vec<_> = output
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text { rect, color, .. } => Some((rect.x, rect.y, *color)),
                _ => None,
            })
            .collect();
        assert_eq!(runs.len(), 3);
        let (x, y, text_color) = runs[2];
        assert_eq!(text_color, Color::rgb(0, 0, 0));
        assert_eq!(runs[0], (x, y + 16.0, text_color));
        assert_eq!(runs[1], (x + 3.0, y + 3.0, Color::rgb(192, 192, 192)));
    }
}
