//! An auto track minimum uses the item's minimum contribution, not min-content.
use super::*;

pub(super) fn is_automatic(track: &GridTrack) -> bool {
    match track {
        GridTrack::Auto | GridTrack::Fraction(_) => true,
        GridTrack::MinMax(minimum, _) => is_automatic(minimum),
        _ => false,
    }
}

pub(super) fn contribution(
    node: &NodeRef,
    style: &ComputedStyle,
    basis: f32,
    min_content: f32,
) -> f32 {
    let margin = style.margin.resolve(basis, style.font_size).horizontal();
    let insets = style.padding.resolve(basis, style.font_size).horizontal()
        + table::resolved_table_borders(node, style, basis).horizontal();
    // Grid 6.6 / 11.5: an explicit zero minimum and scrollable overflow allow the
    // track to shrink below an overflowing descendant's min-content width.
    // Explicit min-content track sizing is deliberately handled separately.
    // https://www.w3.org/TR/css-grid-1/#min-size-auto
    if style.min_width != Length::Auto {
        return resolve_outer_size(
            // Its percentage basis is the as-yet unsized grid area, not the
            // container's whole free space. Cyclic minimum percentages use zero.
            if style.min_width.has_percentage() {
                Length::Px(0.0)
            } else {
                style.min_width.clone()
            },
            basis,
            style.font_size,
            insets,
            style.box_sizing,
        )
        .unwrap_or(insets)
            + margin;
    }
    if style.overflow_axes().0.scrollable() {
        return insets + margin;
    }
    min_content
}
