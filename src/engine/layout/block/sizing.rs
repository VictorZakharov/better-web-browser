//! Used box dimensions for normal blocks and parent-sized items.

use super::super::*;

pub(super) fn resolve_height_constraints(
    style: &ComputedStyle,
    used_content_height: Option<f32>,
    percentage_basis: Option<f32>,
    viewport: RectF,
    vertical_insets: f32,
    margins: ResolvedEdges,
) -> (Option<f32>, f32, Option<f32>) {
    let resolve = |height| {
        resolve_content_height(
            height,
            percentage_basis,
            viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        )
    };
    let mut specified = used_content_height.or_else(|| resolve(style.height));
    if specified.is_none()
        && matches!(style.position, Position::Absolute | Position::Fixed)
        && let Some(basis) = percentage_basis
        && let (Some(top), Some(bottom)) = (
            style.top.resolve(basis, style.font_size),
            style.bottom.resolve(basis, style.font_size),
        )
    {
        // CSS 2.1 section 10.6.4: auto height with definite top/bottom fills the
        // containing block after accounting for margins, padding, and borders.
        specified = Some((basis - top - bottom - margins.vertical() - vertical_insets).max(0.0));
    }
    (
        specified,
        resolve(style.min_height).unwrap_or(0.0),
        resolve(style.max_height),
    )
}

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
