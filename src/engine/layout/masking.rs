//! CSS Masking 1: mask the assembled element, not just its background color.
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn wrap_mask(&mut self, style: &ComputedStyle, bounds: RectF, start: usize) {
        // A single `none` is canonicalized to an empty list. `none, none` is a
        // transparent mask, not an unmasked element. Masks never change hit testing.
        if !self.emit_paint || style.mask_image.0.is_empty() || start >= self.output.items.len() {
            return;
        }
        let layers = style
            .mask_image
            .0
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(layer, image)| self.mask_paint_item(style, image, layer, bounds))
            .collect();
        self.output
            .items
            .insert(start, DisplayItem::BeginCssMask { bounds, layers });
        self.output.items.push(DisplayItem::EndMask { bounds });
    }
}
