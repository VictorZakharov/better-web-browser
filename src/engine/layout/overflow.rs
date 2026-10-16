//! Overflow clip boundaries for retained descendant painting.

use super::*;
use crate::engine::css::Overflow;

#[cfg(test)]
#[path = "tests_overflow.rs"]
mod tests;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn begin_overflow_clip(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
    ) -> Option<usize> {
        // The root's overflow is applied to the viewport. Its horizontal clip
        // must not become a fixed-height element clip as the viewport scrolls.
        let viewport_root = node.tag_name() == Some("html")
            && matches!(style.overflow_axes().1, Overflow::Auto | Overflow::Scroll);
        (self.emit_paint && style.overflow_hidden && !viewport_root).then(|| {
            let index = self.output.items.len();
            self.output.items.push(DisplayItem::BeginClip {
                bounds: RectF::default(),
            });
            index
        })
    }

    pub(super) fn finish_overflow_clip(&mut self, index: Option<usize>, bounds: RectF) {
        let Some(index) = index else {
            return;
        };
        let DisplayItem::BeginClip {
            bounds: begin_bounds,
        } = &mut self.output.items[index]
        else {
            return;
        };
        *begin_bounds = bounds;
        self.output.items.push(DisplayItem::EndClip { bounds });
    }
}
