//! Cached local resampling must preserve the destination's native device phase.
use super::super::tests::{image, pixels, surface::Target};
use super::tests::patterned;
use super::*;
use crate::windows_app::paint_primitives::{paint_alpha_bitmap, paint_alpha_bitmap_size};
use windows_sys::Win32::Graphics::Gdi::{GM_ADVANCED, SetGraphicsMode, SetWorldTransform, XFORM};

#[test]
fn native_scaling_under_affine_world_transform_matches_uncached_pixels() {
    unsafe {
        let source_image = patterned(97, 71);
        let source = Bitmap::create(&source_image, &source_image.bgra, null_mut()).unwrap();
        for matrix in [
            XFORM {
                eM11: 1.25,
                eM12: 0.0,
                eM21: 0.0,
                eM22: 1.25,
                eDx: 13.0,
                eDy: 17.0,
            },
            XFORM {
                eM11: 0.9,
                eM12: 0.4,
                eM21: -0.4,
                eM22: 0.9,
                eDx: 90.0,
                eDy: 11.0,
            },
            XFORM {
                eM11: 1.0,
                eM12: 0.0,
                eM21: 0.3,
                eM22: 1.0,
                eDx: 13.0,
                eDy: 17.0,
            },
        ] {
            let expected = Target::new(&image(640, 400, [77; 4]));
            let actual = Target::new(&image(640, 400, [77; 4]));
            for target in [&expected, &actual] {
                assert_ne!(SetGraphicsMode(target.dc, GM_ADVANCED), 0);
                assert_ne!(SetWorldTransform(target.dc, &matrix), 0);
            }
            let rect = Rect {
                left: 30,
                top: 20,
                right: 287,
                bottom: 149,
            };
            let mut cache = Cache::default();
            // Prime admission without painting the target twice.
            let _ = cache.get("affine", &source_image, source.handle, &rect, actual.dc);
            let cached = cache.get("affine", &source_image, source.handle, &rect, actual.dc);
            paint_alpha_bitmap(expected.dc, source.handle, &source_image, &rect);
            if let Some(cached) = cached {
                paint_alpha_bitmap_size(
                    actual.dc,
                    cached.handle,
                    rect.width() as u32,
                    rect.height() as u32,
                    &rect,
                );
            } else {
                paint_alpha_bitmap(actual.dc, source.handle, &source_image, &rect);
            }
            assert!(
                pixels(&expected.bitmap) == pixels(&actual.bitmap),
                "world={},{},{},{}",
                matrix.eM11,
                matrix.eM12,
                matrix.eM21,
                matrix.eM22
            );
        }
    }
}
