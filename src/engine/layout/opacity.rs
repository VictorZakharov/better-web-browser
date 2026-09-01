//! Group-opacity boundaries for the retained display list.

use super::*;

pub(super) fn display_item_bounds(item: &DisplayItem) -> Option<RectF> {
    match item {
        DisplayItem::BeginOpacity { bounds, .. } | DisplayItem::EndOpacity { bounds } => {
            Some(*bounds)
        }
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::BorderRect { rect, .. }
        | DisplayItem::Text { rect, .. }
        | DisplayItem::Image { rect, .. } => Some(*rect),
        DisplayItem::BackgroundImage { clip_rect, .. } => Some(*clip_rect),
        DisplayItem::Control(spec) => Some(spec.rect),
    }
}

fn union(left: RectF, right: RectF) -> RectF {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    RectF {
        x,
        y,
        width: left.right().max(right.right()) - x,
        height: left.bottom().max(right.bottom()) - y,
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn wrap_opacity(&mut self, item_start: usize, opacity: f32) {
        if opacity >= 1.0 || item_start >= self.output.items.len() {
            return;
        }
        let Some(bounds) = self.output.items[item_start..]
            .iter()
            .filter_map(display_item_bounds)
            .reduce(union)
        else {
            return;
        };
        self.output.items.insert(
            item_start,
            DisplayItem::BeginOpacity {
                bounds,
                opacity: opacity.clamp(0.0, 1.0),
            },
        );
        self.output.items.push(DisplayItem::EndOpacity { bounds });
    }
}
