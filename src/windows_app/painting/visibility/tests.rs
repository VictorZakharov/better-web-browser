use super::*;
use crate::windows_app::paint_primitives::{paint_alpha_bitmap, paint_alpha_image};
use crate::windows_app::platform::*;
use better_web_browser::engine::{DecodedImage, DisplayItem, RectF};
use std::{collections::HashMap, ptr::null_mut};
use windows_sys::Win32::Graphics::Gdi::{GetCurrentObject, OBJ_BITMAP, SelectClipRgn};

mod surface;
use surface::{Surface, image};
mod benchmark;

fn rect() -> Rect {
    Rect {
        left: 7,
        top: 11,
        right: 96,
        bottom: 67,
    }
}

#[test]
fn visibility_rejection_never_changes_clipped_or_transformed_pixels() {
    unsafe {
        let source_image = image(31, 23, false);
        let source = Surface::new(&source_image);
        source.detach();
        for transform in [
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [1.25, 0.0, 0.0, 0.75, -13.375, 9.25],
            [-1.3, 0.0, 0.0, 1.3, 113.5, -3.0],
            [0.8, 0.3, -0.4, 1.1, 33.0, 9.0],
        ] {
            for clip in [
                rect(),
                Rect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                Rect {
                    left: 110,
                    top: 3,
                    right: 140,
                    bottom: 96,
                },
            ] {
                let expected = Surface::new(&image(160, 110, true));
                let actual = Surface::new(&image(160, 110, true));
                for target in [&expected, &actual] {
                    SetViewportOrgEx(target.dc, -3, 5, null_mut());
                    assert!(super::super::affine::set(
                        target.dc,
                        better_web_browser::engine::affine::Affine2D(transform)
                    ));
                    let rgn =
                        CreateRoundRectRgn(clip.left, clip.top, clip.right, clip.bottom, 9, 7);
                    assert_ne!(SelectClipRgn(target.dc, rgn), 0);
                    DeleteObject(rgn);
                }
                paint_alpha_bitmap(expected.dc, source.bitmap, &source_image, &rect());
                if drawable(actual.dc, rect()) {
                    paint_alpha_bitmap(actual.dc, source.bitmap, &source_image, &rect());
                }
                assert_eq!(actual.bytes(), expected.bytes(), "matrix={transform:?}");
            }
        }
    }
}

#[test]
fn shared_image_dc_preserves_pixels_sampling_and_bitmap_lifetimes() {
    unsafe {
        let borrowed = CreateCompatibleDC(null_mut());
        assert!(!borrowed.is_null());
        let original = GetCurrentObject(borrowed, OBJ_BITMAP as u32);
        for opaque in [false, true] {
            let source_image = image(31, 23, opaque);
            let source = Surface::new(&source_image);
            // A bitmap cannot be selected into two memory DCs at the same time.
            source.detach();
            for matrix in [
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                [1.25, 0.0, 0.0, 0.75, -13.375, 9.25],
                [-1.3, 0.0, 0.0, 1.3, 113.5, -3.0],
            ] {
                let expected = Surface::new(&image(160, 110, true));
                let actual = Surface::new(&image(160, 110, true));
                for target in [&expected, &actual] {
                    SetViewportOrgEx(target.dc, -3, 5, null_mut());
                    assert!(super::super::affine::set(
                        target.dc,
                        better_web_browser::engine::affine::Affine2D(matrix)
                    ));
                    IntersectClipRect(target.dc, 10, 8, 115, 81);
                }
                let before = expected.bytes();
                paint_alpha_image(
                    expected.dc,
                    null_mut(),
                    source.bitmap,
                    &source_image,
                    &rect(),
                    opaque,
                    None,
                );
                paint_alpha_image(
                    actual.dc,
                    borrowed,
                    source.bitmap,
                    &source_image,
                    &rect(),
                    opaque,
                    None,
                );
                assert_eq!(
                    actual.bytes(),
                    expected.bytes(),
                    "opaque={opaque}, matrix={matrix:?}"
                );
                assert_ne!(
                    expected.bytes(),
                    before,
                    "reference must actually draw pixels"
                );
                assert_eq!(GetCurrentObject(borrowed, OBJ_BITMAP as u32), original);
            }
        }
        assert_ne!(DeleteDC(borrowed), 0);
    }
}

#[test]
fn invisible_image_does_not_admit_a_native_bitmap() {
    unsafe {
        let target = Surface::new(&image(160, 110, true));
        let images = HashMap::from([("sprite".into(), image(31, 23, true))]);
        let item = DisplayItem::Image {
            rect: RectF {
                x: 7.0,
                y: 11.0,
                width: 89.0,
                height: 56.0,
            },
            clip: None,
            url: "sprite".into(),
            alt: String::new(),
            tint: None,
        };
        let mut bitmaps = crate::windows_app::rendering_resources::ImageBitmaps::default();
        let saved = SaveDC(target.dc);
        IntersectClipRect(target.dc, 120, 90, 140, 105);
        super::super::images::paint(
            &item,
            target.dc,
            null_mut(),
            &images,
            &mut bitmaps,
            0,
            0,
            1.0,
            &Rect {
                left: 0,
                top: 0,
                right: 160,
                bottom: 110,
            },
            None,
        );
        assert!(
            !bitmaps.is_opaque("sprite"),
            "no bitmap admitted behind an excluding clip"
        );
        RestoreDC(target.dc, saved);
        super::super::images::paint(
            &item,
            target.dc,
            null_mut(),
            &images,
            &mut bitmaps,
            0,
            0,
            1.0,
            &Rect {
                left: 0,
                top: 0,
                right: 160,
                bottom: 110,
            },
            None,
        );
        assert!(
            bitmaps.is_opaque("sprite"),
            "revealed image must paint and be admitted"
        );
    }
}
