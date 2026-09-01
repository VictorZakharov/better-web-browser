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
        let mut start = 0;
        let mut depth = 0_usize;
        for (index, item) in items.iter().enumerate() {
            match item {
                DisplayItem::BeginClip { .. } | DisplayItem::BeginOpacity { .. } => depth += 1,
                DisplayItem::EndClip { .. } | DisplayItem::EndOpacity { .. } => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
            let end = index + 1;
            if depth == 0 && (end - start >= ITEMS_PER_CHUNK || end == items.len()) {
                self.push_chunk(items, start..end);
                start = end;
            }
        }
        if start < items.len() {
            self.push_chunk(items, start..items.len());
        }
    }

    fn push_chunk(&mut self, items: &[DisplayItem], range: Range<usize>) {
        let mut top = f32::INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for item in &items[range.clone()] {
            let (item_top, item_bottom) = vertical_bounds(item);
            top = top.min(item_top);
            bottom = bottom.max(item_bottom);
        }
        self.chunks.push(PaintChunk { range, top, bottom });
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
        DisplayItem::BeginClip { bounds }
        | DisplayItem::EndClip { bounds }
        | DisplayItem::BeginOpacity { bounds, .. }
        | DisplayItem::EndOpacity { bounds } => *bounds,
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

    #[test]
    fn opacity_groups_are_never_split_across_paint_chunks() {
        let bounds = RectF {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 3_000.0,
        };
        let mut items = vec![DisplayItem::BeginOpacity {
            bounds,
            opacity: 0.5,
        }];
        items.extend((0..300).map(|index| item(index as f32 * 10.0, 8.0)));
        items.push(DisplayItem::EndOpacity { bounds });
        items.push(item(4_000.0, 10.0));
        let mut index = PaintIndex::default();
        index.rebuild(&items);

        assert_eq!(
            index.visible_ranges(1_000.0, 1_100.0).collect::<Vec<_>>(),
            vec![0..302]
        );
    }

    #[test]
    fn clip_groups_are_never_split_across_paint_chunks() {
        let bounds = RectF {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 3_000.0,
        };
        let mut items = vec![DisplayItem::BeginClip { bounds }];
        items.extend((0..300).map(|index| item(index as f32 * 10.0, 8.0)));
        items.push(DisplayItem::EndClip { bounds });
        items.push(item(4_000.0, 10.0));
        let mut index = PaintIndex::default();
        index.rebuild(&items);

        assert_eq!(
            index.visible_ranges(1_000.0, 1_100.0).collect::<Vec<_>>(),
            vec![0..302]
        );
    }
}
