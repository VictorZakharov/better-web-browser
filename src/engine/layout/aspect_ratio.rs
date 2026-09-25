//! Preferred aspect-ratio transfers for boxes with one automatic dimension.

use super::*;

pub(super) fn height_from_width(
    style: &ComputedStyle,
    natural: Option<(f32, f32)>,
    content_width: f32,
    horizontal_insets: f32,
    vertical_insets: f32,
) -> Option<f32> {
    let (ratio, specified_ratio) = style.aspect_ratio.preferred(natural)?;
    let border_box = specified_ratio && style.box_sizing == BoxSizing::BorderBox;
    let width = content_width + if border_box { horizontal_insets } else { 0.0 };
    let height = width / ratio - if border_box { vertical_insets } else { 0.0 };
    height.is_finite().then_some(height.max(0.0))
}

pub(super) fn width_from_height(
    style: &ComputedStyle,
    natural: Option<(f32, f32)>,
    content_height: f32,
    horizontal_insets: f32,
    vertical_insets: f32,
) -> Option<f32> {
    let (ratio, specified_ratio) = style.aspect_ratio.preferred(natural)?;
    let border_box = specified_ratio && style.box_sizing == BoxSizing::BorderBox;
    let height = content_height + if border_box { vertical_insets } else { 0.0 };
    let width = height * ratio - if border_box { horizontal_insets } else { 0.0 };
    width.is_finite().then_some(width.max(0.0))
}

pub(super) fn automatic_block_height(
    style: &ComputedStyle,
    content_width: f32,
    horizontal_insets: f32,
    vertical_insets: f32,
    natural_content_height: f32,
) -> f32 {
    let Some(ratio_height) = height_from_width(
        style,
        None,
        content_width,
        horizontal_insets,
        vertical_insets,
    ) else {
        return natural_content_height;
    };
    // CSS Sizing 4 §4.3: a non-scroll-container retains its content-based
    // automatic minimum in the ratio-dependent axis. Scroll containers can
    // keep the ratio and expose overflow instead.
    let scroll_container = matches!(
        style.overflow_axes().1,
        Overflow::Auto | Overflow::Scroll | Overflow::Hidden
    );
    if scroll_container {
        ratio_height
    } else {
        natural_content_height.max(ratio_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_box_ratio_transfers_insets_but_auto_natural_ratio_does_not() {
        let mut style = ComputedStyle::initial();
        style.aspect_ratio = AspectRatio::parse("2 / 1").unwrap();
        style.box_sizing = BoxSizing::BorderBox;
        assert_eq!(
            height_from_width(&style, None, 180.0, 20.0, 10.0),
            Some(90.0)
        );
        assert_eq!(
            width_from_height(&style, None, 90.0, 20.0, 10.0),
            Some(180.0)
        );
        style.aspect_ratio = AspectRatio::Auto;
        assert_eq!(
            height_from_width(&style, Some((200.0, 100.0)), 180.0, 20.0, 10.0),
            Some(90.0)
        );
    }

    #[test]
    fn preferred_ratio_respects_content_minimum_except_for_scroll_containers() {
        let mut style = ComputedStyle::initial();
        style.aspect_ratio = AspectRatio::parse("1 / 1").unwrap();
        assert_eq!(
            automatic_block_height(&style, 100.0, 0.0, 0.0, 130.0),
            130.0
        );
        style.apply_overflow("overflow", "auto");
        assert_eq!(
            automatic_block_height(&style, 100.0, 0.0, 0.0, 130.0),
            100.0
        );
    }
}
