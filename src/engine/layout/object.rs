//! Concrete object sizing after replaced-element box layout (CSS Images 3 §4.5–4.6).

use super::*;

/// Fit painted content inside the element's content box without changing its layout bounds.
/// The content box is the painting clip even when CSS overflow is visible.
pub(super) fn image_paint_geometry(
    style: &ComputedStyle,
    content_box: RectF,
    intrinsic_width: f32,
    intrinsic_height: f32,
) -> (RectF, Option<RectF>) {
    if content_box.width <= 0.0
        || content_box.height <= 0.0
        || intrinsic_width <= 0.0
        || intrinsic_height <= 0.0
    {
        return (content_box, None);
    }
    let contain_scale =
        (content_box.width / intrinsic_width).min(content_box.height / intrinsic_height);
    let cover_scale =
        (content_box.width / intrinsic_width).max(content_box.height / intrinsic_height);
    let (width, height) = match style.object_fit {
        ObjectFit::Fill => (content_box.width, content_box.height),
        ObjectFit::Contain => (
            intrinsic_width * contain_scale,
            intrinsic_height * contain_scale,
        ),
        ObjectFit::Cover => (
            intrinsic_width * cover_scale,
            intrinsic_height * cover_scale,
        ),
        ObjectFit::None => (intrinsic_width, intrinsic_height),
        ObjectFit::ScaleDown => {
            if contain_scale < 1.0 {
                (
                    intrinsic_width * contain_scale,
                    intrinsic_height * contain_scale,
                )
            } else {
                (intrinsic_width, intrinsic_height)
            }
        }
    };
    let (offset_x, offset_y) = style.object_position.resolve(
        content_box.width,
        content_box.height,
        width,
        height,
        style.font_size,
    );
    let painted = RectF {
        x: content_box.x + offset_x,
        y: content_box.y + offset_y,
        width,
        height,
    };
    let clip = (painted != content_box).then_some(content_box);
    (painted, clip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_rect() -> RectF {
        RectF {
            x: 5.0,
            y: 7.0,
            width: 200.0,
            height: 100.0,
        }
    }

    #[test]
    fn fill_preserves_the_content_box() {
        let style = ComputedStyle::initial();
        assert_eq!(
            image_paint_geometry(&style, box_rect(), 100.0, 100.0),
            (box_rect(), None)
        );
    }

    #[test]
    fn contain_centers_and_cover_crops_without_resizing_the_element() {
        let mut style = ComputedStyle::initial();
        style.object_fit = ObjectFit::Contain;
        let (painted, clip) = image_paint_geometry(&style, box_rect(), 100.0, 100.0);
        assert_eq!(
            painted,
            RectF {
                x: 55.0,
                y: 7.0,
                width: 100.0,
                height: 100.0
            }
        );
        assert_eq!(clip, Some(box_rect()));
        style.object_fit = ObjectFit::Cover;
        let (painted, clip) = image_paint_geometry(&style, box_rect(), 100.0, 100.0);
        assert_eq!(
            painted,
            RectF {
                x: 5.0,
                y: -43.0,
                width: 200.0,
                height: 200.0
            }
        );
        assert_eq!(clip, Some(box_rect()));
    }

    #[test]
    fn scale_down_never_upscales_and_none_uses_intrinsic_pixels() {
        let mut style = ComputedStyle::initial();
        style.object_fit = ObjectFit::ScaleDown;
        let (painted, _) = image_paint_geometry(&style, box_rect(), 20.0, 10.0);
        assert_eq!((painted.width, painted.height), (20.0, 10.0));
        let (painted, _) = image_paint_geometry(&style, box_rect(), 400.0, 400.0);
        assert_eq!((painted.width, painted.height), (100.0, 100.0));
        style.object_fit = ObjectFit::None;
        let (painted, _) = image_paint_geometry(&style, box_rect(), 400.0, 400.0);
        assert_eq!((painted.width, painted.height), (400.0, 400.0));
    }

    #[test]
    fn object_position_can_align_to_end_with_a_fixed_offset() {
        let mut style = ComputedStyle::initial();
        style.object_fit = ObjectFit::None;
        style.object_position = ObjectPosition::parse("right 10px bottom 20px").unwrap();
        let (painted, _) = image_paint_geometry(&style, box_rect(), 50.0, 40.0);
        assert_eq!((painted.x, painted.y), (145.0, 47.0));
    }
}
