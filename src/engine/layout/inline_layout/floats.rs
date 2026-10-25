//! Floats anchored among inline content (CSS 2.1 §9.5.1).
//!
//! A float reached by line breaking goes on the current line when it fits beside the
//! content already there (rules 6-8: never above the current line, as high as possible),
//! and the line is shortened around it. Otherwise it waits below the current line, and
//! later floats queue behind it so that source order is kept (rule 5).
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// Returns whether the float was placed on the current line. `area` is the formatting
    /// context's `(x, line top, width)`; `line` is the current line's `(used, available,
    /// empty)`. A float reached before any content on its line is placed at once, moving
    /// down past earlier floats as needed, like a float between blocks.
    pub(super) fn reach_inline_float(
        &mut self,
        node: &NodeRef,
        (x, y, width): (f32, f32, f32),
        (line_width, available, empty): (f32, f32, bool),
        deferred: &mut Vec<NodeRef>,
    ) -> bool {
        let fits =
            empty || line_width + self.float_outer_width(node, width) <= available + 1.0 / 64.0;
        if !deferred.is_empty() || !fits {
            deferred.push(node.clone());
            return false;
        }
        self.layout_float(node, x, y, width, None);
        true
    }

    /// Places the floats that did not fit on the line that just ended.
    pub(super) fn place_deferred_floats(
        &mut self,
        deferred: &mut Vec<NodeRef>,
        x: f32,
        y: f32,
        width: f32,
    ) {
        for node in deferred.drain(..) {
            self.layout_float(&node, x, y, width, None);
        }
    }
}
