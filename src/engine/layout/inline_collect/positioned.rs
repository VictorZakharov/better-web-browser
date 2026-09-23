use super::*;

pub(super) fn relative_replaced_offset(
    style: &ComputedStyle,
    containing_block: InlineContainingBlock,
) -> (f32, f32) {
    if style.position != Position::Relative {
        return (0.0, 0.0);
    }
    // CSS 2.2 §9.4.3: relative offsets move ink and hit geometry, not the
    // element's original normal-flow space. Percentages use the containing
    // block, not the replaced element's own (often much larger) dimensions.
    let horizontal = style
        .left
        .resolve(containing_block.width, style.font_size)
        .or_else(|| {
            style
                .right
                .resolve(containing_block.width, style.font_size)
                .map(|value| -value)
        })
        .unwrap_or(0.0);
    let vertical = style
        .top
        .resolve(containing_block.height.unwrap_or(0.0), style.font_size)
        .or_else(|| {
            style
                .bottom
                .resolve(containing_block.height.unwrap_or(0.0), style.font_size)
                .map(|value| -value)
        })
        .unwrap_or(0.0);
    (horizontal, vertical)
}
