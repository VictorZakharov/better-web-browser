//! Inline-size selection for normal blocks and items sized by a parent layout algorithm.

use super::super::*;

pub(super) fn resolve_used_border_box_width(
    style: &ComputedStyle,
    containing_width: f32,
    horizontal_insets: f32,
    margins: ResolvedEdges,
    automatic_width: f32,
    used_inline_size: Option<UsedInlineSize>,
) -> f32 {
    let mut width = used_inline_size
        .map(|size| (size.outer - margins.horizontal()).max(0.0))
        .or_else(|| {
            resolve_outer_size(
                style.width,
                containing_width,
                style.font_size,
                horizontal_insets,
                style.box_sizing,
            )
        })
        .unwrap_or(automatic_width);
    if used_inline_size.is_some() {
        return width;
    }
    if let Some(maximum) = resolve_outer_size(
        style.max_width,
        containing_width,
        style.font_size,
        horizontal_insets,
        style.box_sizing,
    ) {
        width = width.min(maximum);
    }
    if let Some(minimum) = resolve_outer_size(
        style.min_width,
        containing_width,
        style.font_size,
        horizontal_insets,
        style.box_sizing,
    ) {
        width = width.max(minimum);
    }
    width
}
