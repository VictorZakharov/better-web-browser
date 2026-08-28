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
                && previous.items == next.items)
        {
            return Self::full(previous.items.len().max(next.items.len()));
        }

        let prefix = previous
            .items
            .iter()
            .zip(&next.items)
            .take_while(|(left, right)| left == right)
            .count();
        let suffix_limit = previous.items.len().min(next.items.len()) - prefix;
        let suffix = previous
            .items
            .iter()
            .rev()
            .zip(next.items.iter().rev())
            .take(suffix_limit)
            .take_while(|(left, right)| left == right)
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

fn item_bounds(item: &DisplayItem) -> RectF {
    match item {
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::BorderRect { rect, .. }
        | DisplayItem::Text { rect, .. }
        | DisplayItem::Image { rect, .. } => *rect,
        DisplayItem::BackgroundImage { clip_rect, .. } => *clip_rect,
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
            items,
            content_height: 100.0,
            background: Color::WHITE,
            forms: HashMap::new(),
            node_bounds: HashMap::new(),
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
            },
            color: Color::BLACK,
            link: None,
            node_id: None,
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
}
