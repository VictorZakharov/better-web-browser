use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn constrain(
    width: f32,
    height: f32,
    auto_width: bool,
    auto_height: bool,
    style: &ComputedStyle,
    containing: InlineContainingBlock,
    viewport: RectF,
) -> (f32, f32) {
    // CSS 2.2 10.4/10.7: min/max constraints apply to replaced elements too.
    // With both dimensions auto, preserve the intrinsic ratio where constraints permit it.
    let min_width = style
        .min_width
        .resolve(containing.width, style.font_size)
        .unwrap_or(0.0)
        .max(0.0);
    let max_width = style
        .max_width
        .resolve(containing.width, style.font_size)
        .unwrap_or(f32::INFINITY)
        .max(min_width);
    let min_height = resolve_height_value(
        style.min_height,
        containing.height,
        viewport,
        style.font_size,
    )
    .unwrap_or(0.0)
    .max(0.0);
    let max_height = resolve_height_value(
        style.max_height,
        containing.height,
        viewport,
        style.font_size,
    )
    .unwrap_or(f32::INFINITY)
    .max(min_height);
    if auto_width && auto_height && width > 0.0 && height > 0.0 {
        let low = (min_width / width).max(min_height / height);
        let high = (max_width / width).min(max_height / height);
        let factor = if low <= high {
            1.0_f32.clamp(low, high)
        } else {
            low
        };
        return (
            (width * factor).clamp(min_width, max_width),
            (height * factor).clamp(min_height, max_height),
        );
    }
    let used_width = width.clamp(min_width, max_width);
    let used_height = if auto_height && width > 0.0 {
        height * used_width / width
    } else {
        height
    };
    let used_height = used_height.clamp(min_height, max_height);
    let used_width = if auto_width && height > 0.0 {
        width * used_height / height
    } else {
        used_width
    };
    (used_width.clamp(min_width, max_width), used_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intrinsic_ratio_obeys_both_axes_and_conflicting_bounds() {
        let page = Page::parse("<img>", "https://example.com/");
        let image = page.dom.elements_named("img").next().unwrap();
        let styles = page.style(400.0);
        let containing = InlineContainingBlock {
            width: 400.0,
            height: Some(300.0),
        };
        for (min_w, max_w, min_h, max_h, expected) in [
            (0.0, 40.0, 0.0, 100.0, (40.0, 20.0)),
            (0.0, 100.0, 0.0, 10.0, (20.0, 10.0)),
            (200.0, 300.0, 0.0, 300.0, (200.0, 100.0)),
            (200.0, 300.0, 0.0, 30.0, (200.0, 30.0)),
        ] {
            let mut style = styles.get(&image).clone();
            style.min_width = Length::Px(min_w);
            style.max_width = Length::Px(max_w);
            style.min_height = Length::Px(min_h);
            style.max_height = Length::Px(max_h);
            assert_eq!(
                constrain(
                    100.0,
                    50.0,
                    true,
                    true,
                    &style,
                    containing,
                    RectF::default()
                ),
                expected
            );
        }
    }
}
