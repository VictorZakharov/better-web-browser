use super::*;
#[test]
fn coverage_premultiplies_pixels_without_replacing_the_backdrop() {
    let rect = Rect {
        left: 5,
        top: 8,
        right: 7,
        bottom: 9,
    };
    let image = DecodedImage {
        width: 2,
        height: 1,
        bgra: vec![0, 0, 0, 0, 0, 0, 0, 128].into(),
    };
    let mut pixels = vec![20, 40, 80, 255, 20, 40, 80, 255];
    apply_mask(&mut pixels, rect, rect, &image);
    assert_eq!(pixels, [0, 0, 0, 0, 10, 20, 40, 128]);
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetPixel(dc: Hdc, x: i32, y: i32) -> u32;
}

#[test]
fn offscreen_mask_and_opacity_nesting_preserves_colored_backdrops_and_dirty_clips() {
    use super::super::super::paint_primitives::fill_color_rect;
    use super::super::groups::PaintGroups;
    use better_web_browser::engine::DisplayItem;
    use std::collections::HashMap;
    unsafe {
        let surface = DecodedImage {
            width: 8,
            height: 4,
            bgra: vec![0; 8 * 4 * 4].into(),
        };
        let dc = CreateCompatibleDC(null_mut());
        assert!(!dc.is_null());
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &bitmap_info(&surface), 0, &mut bits, null_mut(), 0);
        assert!(!bitmap.is_null());
        let previous = SelectObject(dc, bitmap);
        let content = Rect {
            left: 0,
            top: 0,
            right: 8,
            bottom: 4,
        };
        let dirty = Rect {
            left: 2,
            top: 0,
            right: 6,
            bottom: 4,
        };
        let bounds = RectF {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 4.0,
        };
        let images = HashMap::from([(
            "mask".into(),
            DecodedImage {
                width: 2,
                height: 1,
                bgra: vec![0, 0, 0, 0, 128, 128, 128, 128].into(),
            },
        )]);
        for outer_mask in [true, false] {
            fill_color_rect(dc, &content, 0xff0000); // Blue backdrop in COLORREF.
            let mut groups = PaintGroups::default();
            let mask = DisplayItem::BeginMask {
                bounds,
                url: "mask".into(),
            };
            let opacity = DisplayItem::BeginOpacity {
                bounds,
                opacity: 0.5,
            };
            let (a, b) = if outer_mask {
                (&mask, &opacity)
            } else {
                (&opacity, &mask)
            };
            for item in [a, b] {
                assert!(groups.handle(item, dc, &images, 0, 0, 1.0, &content, &dirty));
            }
            fill_color_rect(groups.dc(dc), &content, 0x0000ff); // Red child.
            let end_mask = DisplayItem::EndMask { bounds };
            let end_opacity = DisplayItem::EndOpacity { bounds };
            let ends = if outer_mask {
                [&end_opacity, &end_mask]
            } else {
                [&end_mask, &end_opacity]
            };
            for item in ends {
                assert!(groups.handle(item, dc, &images, 0, 0, 1.0, &content, &dirty));
            }
            assert_eq!(groups.dc(dc), dc);
            assert_eq!(GetPixel(dc, 1, 1), 0xff0000); // Outside dirty region.
            assert_eq!(GetPixel(dc, 3, 1), 0xff0000); // Zero mask alpha.
            assert_eq!(GetPixel(dc, 6, 1), 0xff0000);
            let pixel = GetPixel(dc, 5, 1);
            let red = pixel & 255;
            let blue = (pixel >> 16) & 255;
            assert!(
                (63..=65).contains(&red) && (190..=192).contains(&blue),
                "mask outer={outer_mask}, {pixel:x}"
            );
        }
        SelectObject(dc, previous);
        DeleteObject(bitmap);
        DeleteDC(dc);
    }
}

#[test]
fn cropped_mask_sampling_keeps_original_element_coordinates() {
    let full = Rect {
        left: 10,
        top: 20,
        right: 14,
        bottom: 21,
    };
    let dirty = Rect {
        left: 12,
        top: 20,
        right: 14,
        bottom: 21,
    };
    let image = DecodedImage {
        width: 4,
        height: 1,
        bgra: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 255].into(),
    };
    let mut pixels = vec![100; 8];
    apply_mask(&mut pixels, dirty, full, &image);
    assert_eq!(pixels, [50, 50, 50, 128, 100, 100, 100, 255]);
}
