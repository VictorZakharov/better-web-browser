//! Conservative damage detection between retained display lists.

use super::layout::{DisplayItem, LayoutOutput, RectF};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayListDamage {
    pub rect: Option<RectF>,
    pub changed_items: usize,
    pub full_repaint: bool,
}

impl DisplayListDamage {
    pub fn between(previous: &LayoutOutput, next: &LayoutOutput) -> Self {
        if previous.items.is_empty()
            || previous.background != next.background
            || (!same_number(previous.content_height, next.content_height)
                && previous.items.len() == next.items.len()
                && previous
                    .items
                    .iter()
                    .zip(&next.items)
                    .all(|(left, right)| same_display_item(left, right)))
        {
            return Self::full(previous.items.len().max(next.items.len()));
        }

        let prefix = previous
            .items
            .iter()
            .zip(&next.items)
            .take_while(|(left, right)| same_display_item(left, right))
            .count();
        let suffix_limit = previous.items.len().min(next.items.len()) - prefix;
        let suffix = previous
            .items
            .iter()
            .rev()
            .zip(next.items.iter().rev())
            .take(suffix_limit)
            .take_while(|(left, right)| same_display_item(left, right))
            .count();
        let previous_changed = &previous.items[prefix..previous.items.len() - suffix];
        let next_changed = &next.items[prefix..next.items.len() - suffix];
        let changed_items = previous_changed.len().max(next_changed.len());

        if changed_items == 0 {
            return Self::default();
        }

        let rect = previous_changed
            .iter()
            .chain(next_changed)
            .map(item_bounds)
            .reduce(union_rects);
        let total_items = previous.items.len().max(next.items.len());
        if rect.is_none() || changed_items.saturating_mul(4) >= total_items.saturating_mul(3) {
            Self::full(changed_items)
        } else {
            Self {
                rect,
                changed_items,
                full_repaint: false,
            }
        }
    }

    pub fn full(changed_items: usize) -> Self {
        Self {
            rect: None,
            changed_items,
            full_repaint: true,
        }
    }

    pub fn is_empty(self) -> bool {
        !self.full_repaint && self.rect.is_none()
    }
}

/// Item equality that recognises a retained text run by its shared glyphs before comparing
/// them. Retained layout and the presentation splice share glyph runs between successive lists,
/// so unchanged runs compare by pointer instead of by every glyph position.
pub fn same_display_item(a: &DisplayItem, b: &DisplayItem) -> bool {
    match (a, b) {
        (
            DisplayItem::Text {
                rect,
                text,
                font,
                color,
                link,
                node_id,
                color_owner,
                raster_run_id,
                glyphs,
            },
            DisplayItem::Text {
                rect: other_rect,
                text: other_text,
                font: other_font,
                color: other_color,
                link: other_link,
                node_id: other_node,
                color_owner: other_owner,
                raster_run_id: other_run,
                glyphs: other_glyphs,
            },
        ) => {
            rect == other_rect
                && color == other_color
                && raster_run_id == other_run
                && node_id == other_node
                && color_owner == other_owner
                && text == other_text
                && link == other_link
                && font == other_font
                && (std::sync::Arc::ptr_eq(glyphs, other_glyphs) || glyphs == other_glyphs)
        }
        _ => a == b,
    }
}

fn item_bounds(item: &DisplayItem) -> RectF {
    match item {
        DisplayItem::NodeBoundary { .. } | DisplayItem::PaintBoundary { .. } => RectF::default(),
        DisplayItem::BeginClip { bounds }
        | DisplayItem::EndClip { bounds }
        | DisplayItem::BeginOpacity { bounds, .. }
        | DisplayItem::BeginMask { bounds, .. }
        | DisplayItem::BeginCssMask { bounds, .. }
        | DisplayItem::EndMask { bounds }
        | DisplayItem::EndOpacity { bounds } => *bounds,
        DisplayItem::BeginTransform { bounds, .. } | DisplayItem::EndTransform { bounds } => {
            *bounds
        }
        DisplayItem::EmbeddedFrame { rect, .. }
        | DisplayItem::SolidRect { rect, .. }
        | DisplayItem::BorderRect { rect, .. }
        | DisplayItem::Text { rect, .. } => *rect,
        DisplayItem::Image { rect, clip, .. } => clip.unwrap_or(*rect),
        DisplayItem::BackgroundImage { clip_rect, .. }
        | DisplayItem::CssGradient { clip_rect, .. } => *clip_rect,
        DisplayItem::CssBoxShadow { bounds, .. } => *bounds,
        DisplayItem::Control(spec) => spec.rect,
    }
}

fn union_rects(left: RectF, right: RectF) -> RectF {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    RectF {
        x,
        y,
        width: left.right().max(right.right()) - x,
        height: left.bottom().max(right.bottom()) - y,
    }
}

fn same_number(left: f32, right: f32) -> bool {
    (left - right).abs() < f32::EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::css::Color;
    use crate::engine::layout::{FontSpec, LayoutOutput};
    use std::collections::HashMap;

    fn output(items: Vec<DisplayItem>) -> LayoutOutput {
        LayoutOutput {
            fixed_layers: Default::default(),
            transformed: Default::default(),
            fragments: Default::default(),
            sticky_offsets: Default::default(),
            sticky_layers: Default::default(),
            scroll_boxes: Default::default(),
            clip_paths: Default::default(),
            hit_excluded: Default::default(),
            items,
            content_height: 100.0,
            background: Color::WHITE,
            forms: HashMap::new(),
            node_bounds: HashMap::new(),
            resize_boxes: HashMap::new(),
            node_paint_order: Vec::new(),
        }
    }

    fn text(y: f32, value: &str) -> DisplayItem {
        DisplayItem::Text {
            rect: RectF {
                x: 10.0,
                y,
                width: 80.0,
                height: 20.0,
            },
            text: value.into(),
            font: FontSpec {
                family: "sans-serif".into(),
                size: 16.0,
                weight: 400,
                italic: false,
                underline: false,
                letter_spacing: 0.0,
                word_spacing: 0.0,
                rtl: false,
                kerning: true,
                variants: Default::default(),
                features: Default::default(),
            },
            color: Color::BLACK,
            link: None,
            node_id: None,
            color_owner: None,
            raster_run_id: 0,
            glyphs: Default::default(),
        }
    }

    #[test]
    fn isolates_a_small_middle_change() {
        let previous = output(vec![
            text(0.0, "before"),
            text(20.0, "old"),
            text(40.0, "after"),
        ]);
        let next = output(vec![
            text(0.0, "before"),
            text(20.0, "new"),
            text(40.0, "after"),
        ]);

        let damage = DisplayListDamage::between(&previous, &next);

        assert!(!damage.full_repaint);
        assert_eq!(damage.changed_items, 1);
        assert_eq!(damage.rect.unwrap().y, 20.0);
    }

    #[test]
    fn falls_back_for_broad_or_background_changes() {
        let previous = output(vec![text(0.0, "one")]);
        let mut next = output(vec![text(0.0, "two")]);
        assert!(DisplayListDamage::between(&previous, &next).full_repaint);

        next = output(vec![text(0.0, "one")]);
        next.background = Color::BLACK;
        assert!(DisplayListDamage::between(&previous, &next).full_repaint);
    }

    #[test]
    fn glyph_runs_compare_by_content_when_not_shared() {
        let run = |x: f32| {
            let mut item = text(0.0, "run");
            if let DisplayItem::Text { glyphs, .. } = &mut item {
                *glyphs = std::sync::Arc::from([crate::engine::layout::PositionedGlyph {
                    raster_id: 1,
                    x,
                    y: 0.0,
                    width: 8.0,
                    height: 16.0,
                    color: false,
                }]);
            }
            item
        };
        let first = run(1.0);
        assert!(same_display_item(&first, &first.clone()));
        assert!(same_display_item(&first, &run(1.0)));
        assert!(!same_display_item(&first, &run(2.0)));
        assert!(!same_display_item(&first, &text(0.0, "run")));
    }
}
