use super::super::super::tests::{image, pixels, surface::Target};
use super::super::device_phase_tests::DeviceTarget;
use super::super::tests::patterned;
use super::*;
use crate::windows_app::paint_primitives::{paint_alpha_bitmap, paint_alpha_bitmap_size};
use windows_sys::Win32::Graphics::Gdi::{SelectClipRgn, SetWindowOrgEx};

mod benchmark;
mod cache;

fn destination() -> Rect {
    Rect {
        left: 7,
        top: 11,
        right: 96,
        bottom: 67,
    }
}

unsafe fn setup(dc: Hdc, matrix: [f32; 6], clipped: bool) {
    let matrix = XFORM {
        eM11: matrix[0],
        eM12: matrix[1],
        eM21: matrix[2],
        eM22: matrix[3],
        eDx: matrix[4],
        eDy: matrix[5],
    };
    assert_ne!(SetGraphicsMode(dc, GM_ADVANCED), 0);
    assert_ne!(SetWorldTransform(dc, &matrix), 0);
    assert_ne!(SetViewportOrgEx(dc, -3, 5, null_mut()), 0);
    if clipped {
        let region = CreateRoundRectRgn(19, 23, 147, 97, 19, 21);
        assert_ne!(SelectClipRgn(dc, region), 0);
        DeleteObject(region);
        IntersectClipRect(dc, 10, 8, 115, 81);
    }
}

unsafe fn replay(dc: Hdc, baked: &Bitmap, plan: Reflection) {
    let identity = crate::windows_app::painting::affine::IdentityScope::new(dc).unwrap();
    paint_alpha_bitmap_size(dc, baked.handle, baked.width, baked.height, &plan.bounds());
    drop(identity);
}

#[test]
fn horizontal_native_baking_matches_every_byte_across_scales_phases_and_clips() {
    unsafe {
        let source_image = patterned(31, 23);
        let source = Bitmap::create(&source_image, &source_image.bgra, null_mut()).unwrap();
        for sx in [-0.5, -0.75, -1.0, -1.25, -1.3, -2.1] {
            for sy in [0.5, 0.75, 1.0, 1.25, 1.3, 2.1] {
                for phase in [0.0, 0.125, 0.375, 0.5, 0.75, 0.999] {
                    for clipped in [false, true] {
                        let expected = Target::new(&image(160, 110, [77; 4]));
                        let actual = Target::new(&image(160, 110, [77; 4]));
                        let matrix = [sx, 0.0, 0.0, sy, 145.0 + phase, 3.0 + phase];
                        setup(expected.dc, matrix, clipped);
                        setup(actual.dc, matrix, clipped);
                        let plan = Reflection::new(actual.dc, destination()).unwrap();
                        let bounds = plan.bounds();
                        let baked = super::super::raster::resample(
                            source.handle,
                            &source_image,
                            bounds.width(),
                            bounds.height(),
                            bounds.width() as usize * bounds.height() as usize * 4,
                            actual.dc,
                            Some(plan),
                        )
                        .unwrap();
                        paint_alpha_bitmap(
                            expected.dc,
                            source.handle,
                            &source_image,
                            &destination(),
                        );
                        replay(actual.dc, &baked, plan);
                        assert_eq!(
                            pixels(&actual.bitmap),
                            pixels(&expected.bitmap),
                            "matrix={matrix:?}, clipped={clipped}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn horizontal_cache_matches_device_bitmaps_with_fractional_phase_and_viewport_offset() {
    unsafe {
        let source_image = patterned(153, 97);
        let source = Bitmap::create(&source_image, &source_image.bgra, null_mut()).unwrap();
        for matrix in [
            [-1.25, 0.0, 0.0, 1.25, 513.125, 17.75],
            [-1.3, 0.0, 0.0, 0.75, 413.375, -7.25],
            [-2.125, 0.0, 0.0, 1.75, 613.125, 1.875],
            [-0.8, 0.0, 0.0, 1.2, 521.99, -19.99],
        ] {
            for origin in [(30, 20), (-13, -17), (0, 0)] {
                let expected = DeviceTarget::new(640, 400);
                let actual = DeviceTarget::new(640, 400);
                setup(expected.dc, matrix, true);
                setup(actual.dc, matrix, true);
                let rect = Rect {
                    left: origin.0,
                    top: origin.1,
                    right: origin.0 + 257,
                    bottom: origin.1 + 129,
                };
                let plan = Reflection::new(actual.dc, rect).unwrap();
                let mut cache = Cache::default();
                assert!(
                    cache
                        .reflected("device", &source_image, source.handle, plan, actual.dc)
                        .is_none()
                );
                let baked = cache
                    .reflected("device", &source_image, source.handle, plan, actual.dc)
                    .unwrap();
                paint_alpha_bitmap(expected.dc, source.handle, &source_image, &rect);
                let identity =
                    crate::windows_app::painting::affine::IdentityScope::new(actual.dc).unwrap();
                let bounds = plan.bounds();
                paint_alpha_bitmap_size(
                    actual.dc,
                    baked.handle,
                    bounds.width() as u32,
                    bounds.height() as u32,
                    &bounds,
                );
                drop(identity);
                assert_eq!(
                    actual.pixels(),
                    expected.pixels(),
                    "matrix={matrix:?}, origin={origin:?}"
                );
                assert_eq!(cache.entries.len(), 1);
            }
        }
    }
}

#[test]
fn unsupported_transform_mapping_and_extent_decline_without_mutating_the_dc() {
    unsafe {
        let target = Target::new(&image(160, 110, [77; 4]));
        for matrix in [
            [1.25, 0.0, 0.0, 1.25, 13.0, 17.0],
            [-1.25, 0.0, 0.0, -0.75, 113.375, 93.25],
            [-1.25, 0.1, 0.0, 0.75, 113.375, 93.25],
            [-1.25, 0.0, 0.1, 0.75, 113.375, 93.25],
            [-1.25, 0.0, 0.0, 0.75, 2_097_152.0, 93.25],
        ] {
            setup(target.dc, matrix, true);
            let mut before: XFORM = std::mem::zeroed();
            let mut after: XFORM = std::mem::zeroed();
            GetWorldTransform(target.dc, &mut before);
            assert!(Reflection::new(target.dc, destination()).is_none());
            GetWorldTransform(target.dc, &mut after);
            assert_eq!(
                [
                    before.eM11,
                    before.eM12,
                    before.eM21,
                    before.eM22,
                    before.eDx,
                    before.eDy
                ],
                [
                    after.eM11, after.eM12, after.eM21, after.eM22, after.eDx, after.eDy
                ]
            );
        }
        setup(target.dc, [-1.25, 0.0, 0.0, 1.25, 145.0, 3.0], false);
        assert!(Reflection::new(target.dc, destination()).is_some());
        SetWindowOrgEx(target.dc, 1, 0, null_mut());
        assert!(Reflection::new(target.dc, destination()).is_none());
    }
}
