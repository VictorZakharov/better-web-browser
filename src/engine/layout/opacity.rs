//! Group-opacity boundaries for the retained display list.

use super::*;

pub(super) fn display_item_bounds(item: &DisplayItem) -> Option<RectF> {
    match item {
        DisplayItem::NodeBoundary { .. } | DisplayItem::PaintBoundary { .. } => None,
        DisplayItem::BeginClip { bounds }
        | DisplayItem::EndClip { bounds }
        | DisplayItem::BeginOpacity { bounds, .. }
        | DisplayItem::BeginMask { bounds, .. }
        | DisplayItem::BeginCssMask { bounds, .. }
        | DisplayItem::EndMask { bounds }
        | DisplayItem::EndOpacity { bounds } => Some(*bounds),
        DisplayItem::BeginTransform { bounds, .. } | DisplayItem::EndTransform { bounds } => {
            Some(*bounds)
        }
        DisplayItem::CssBoxShadow { bounds, .. } => Some(*bounds),
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::BorderRect { rect, .. }
        | DisplayItem::Text { rect, .. }
        | DisplayItem::EmbeddedFrame { rect, .. } => Some(*rect),
        DisplayItem::Image { rect, clip, .. } => Some(clip.unwrap_or(*rect)),
        DisplayItem::BackgroundImage { clip_rect, .. }
        | DisplayItem::CssGradient { clip_rect, .. } => Some(*clip_rect),
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

// A retained transform already records the ink bounds in its parent's coordinate
// space. Including its untransformed children again inflates opacity buffers.
pub(super) fn range_bounds(items: &[DisplayItem]) -> Option<RectF> {
    let mut depth = 0usize;
    let mut result = None;
    for item in items {
        if matches!(item, DisplayItem::EndTransform { .. }) {
            depth = depth.saturating_sub(1);
            continue;
        }
        if depth == 0
            && let Some(rect) = display_item_bounds(item)
        {
            result = Some(result.map_or(rect, |old| union(old, rect)));
        }
        if matches!(item, DisplayItem::BeginTransform { .. }) {
            depth += 1;
        }
    }
    result
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn wrap_opacity(&mut self, item_start: usize, opacity: f32) {
        if !self.emit_paint || opacity >= 1.0 || item_start >= self.output.items.len() {
            return;
        }
        let Some(bounds) = range_bounds(&self.output.items[item_start..]) else {
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
