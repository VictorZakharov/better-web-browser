//! Conservative vertical index for the retained display list.

use better_web_browser::engine::DisplayItem;
use std::ops::Range;

const ITEMS_PER_CHUNK: usize = 128;

#[derive(Default)]
pub(super) struct PaintIndex {
    chunks: Vec<PaintChunk>,
}

struct PaintChunk {
    range: Range<usize>,
    top: f32,
    bottom: f32,
}

impl PaintIndex {
    pub(super) fn rebuild(&mut self, items: &[DisplayItem]) {
        self.chunks.clear();
        for start in (0..items.len()).step_by(ITEMS_PER_CHUNK) {
            let end = (start + ITEMS_PER_CHUNK).min(items.len());
            let mut top = f32::INFINITY;
            let mut bottom = f32::NEG_INFINITY;
            for item in &items[start..end] {
                let (item_top, item_bottom) = vertical_bounds(item);
                top = top.min(item_top);
                bottom = bottom.max(item_bottom);
            }
            self.chunks.push(PaintChunk {
                range: start..end,
                top,
                bottom,
            });
        }
    }

    pub(super) fn visible_ranges(
        &self,
        top: f32,
        bottom: f32,
    ) -> impl Iterator<Item = Range<usize>> + '_ {
        self.chunks
            .iter()
            .filter(move |chunk| chunk.bottom >= top && chunk.top <= bottom)
            .map(|chunk| chunk.range.clone())
    }
}

fn vertical_bounds(item: &DisplayItem) -> (f32, f32) {
    let rect = match item {
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::BorderRect { rect, .. }
        | DisplayItem::Text { rect, .. }
        | DisplayItem::Image { rect, .. } => *rect,
        DisplayItem::BackgroundImage { clip_rect, .. } => *clip_rect,
        DisplayItem::Control(spec) => spec.rect,
    };
    let top = rect.y.min(rect.bottom());
    let bottom = rect.y.max(rect.bottom());
    if top.is_finite() && bottom.is_finite() {
        (top, bottom)
    } else {
        (f32::NEG_INFINITY, f32::INFINITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::engine::{RectF, css::Color};

    fn item(y: f32, height: f32) -> DisplayItem {
        DisplayItem::SolidRect {
            rect: RectF {
                x: 0.0,
                y,
                width: 100.0,
                height,
            },
            color: Color::BLACK,
            radius: 0.0,
        }
    }

    #[test]
    fn long_pages_only_visit_nearby_display_list_chunks() {
        let items = (0..4_096)
            .map(|index| item(index as f32 * 20.0, 10.0))
            .collect::<Vec<_>>();
        let mut index = PaintIndex::default();
        index.rebuild(&items);

        let ranges = index.visible_ranges(40_000.0, 40_600.0).collect::<Vec<_>>();
        let candidates = ranges.iter().map(Range::len).sum::<usize>();

        assert!(candidates <= ITEMS_PER_CHUNK * 2, "{ranges:?}");
        assert!(ranges.windows(2).all(|pair| pair[0].end <= pair[1].start));
    }

    #[test]
    fn tall_items_keep_their_original_paint_chunk_visible() {
        let mut items = vec![item(0.0, 100_000.0)];
        items.extend((1..256).map(|index| item(index as f32 * 20.0, 10.0)));
        let mut index = PaintIndex::default();
        index.rebuild(&items);

        let ranges = index.visible_ranges(80_000.0, 80_600.0).collect::<Vec<_>>();

        assert_eq!(ranges.first(), Some(&(0..ITEMS_PER_CHUNK)));
    }
}
