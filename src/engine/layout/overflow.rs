//! Overflow clip boundaries for retained descendant painting.

use super::*;

#[cfg(test)]
#[path = "tests_overflow.rs"]
mod tests;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn begin_overflow_clip(&mut self, style: &ComputedStyle) -> Option<usize> {
        style.overflow_hidden.then(|| {
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
