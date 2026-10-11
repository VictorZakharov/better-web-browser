//! Device-phase compatibility: never admit a faster draw with different pixels.
use super::super::tests::{image, pixels, surface::Target};
use super::tests::patterned;
use super::*;
use crate::windows_app::paint_primitives::paint_alpha_bitmap;
use windows_sys::Win32::Graphics::Gdi::{
    GM_ADVANCED, GetClipBox, GetViewportOrgEx, GetWorldTransform, SelectClipRgn, SetGraphicsMode,
    SetWorldTransform, XFORM,
};

mod benchmark;

unsafe fn flattened_paint(
    dc: Hdc,
    source: &Bitmap,
    image: &DecodedImage,
    rect: Rect,
    cached: bool,
) {
    let plan = crate::windows_app::painting::affine::ScaledDestination::new(dc, rect).unwrap();
    let rect = plan.rect();
    if cached {
        let mut cache = Cache::default();
        let _ = cache.get("device-phase", image, source.handle, &rect, dc);
        let scaled = cache
            .get("device-phase", image, source.handle, &rect, dc)
            .unwrap();
        crate::windows_app::paint_primitives::paint_alpha_bitmap_size(
            dc,
            scaled.handle,
            rect.width() as u32,
            rect.height() as u32,
            &rect,
        );
    } else {
        paint_alpha_bitmap(dc, source.handle, image, &rect);
    }
    drop(plan);
}

#[test]
fn native_axis_world_transform_flattening_preserves_every_destination_byte() {
    unsafe {
        let source_image = patterned(97, 71);
        let source = Bitmap::create(&source_image, &source_image.bgra, null_mut()).unwrap();
        for [sx, sy, dx, dy] in [
            [1.25, 1.25, 13.0, 17.0],
            [1.3, 0.75, -13.375, -7.25],
            [2.125, 1.75, 0.125, 1.875],
            [0.8, 1.2, 21.99, -19.99],
            [1.25, 1.25, 0.4999, -0.5001],
            [1.25, 1.25, 0.5001, -0.4999],
            [1.001, 0.999, -0.5, 0.5],
        ] {
            for origin in [(30, 20), (-13, -17), (0, 0)] {
                for (clipped, cached) in
                    [(false, false), (true, false), (false, true), (true, true)]
                {
                    let expected = Target::new(&image(640, 400, [77; 4]));
                    let actual = Target::new(&image(640, 400, [77; 4]));
                    let matrix = XFORM {
                        eM11: sx,
                        eM12: 0.0,
                        eM21: 0.0,
                        eM22: sy,
                        eDx: dx,
                        eDy: dy,
                    };
                    for target in [&expected, &actual] {
                        assert_ne!(SetGraphicsMode(target.dc, GM_ADVANCED), 0);
                        assert_ne!(SetWorldTransform(target.dc, &matrix), 0);
                        SetViewportOrgEx(target.dc, -3, -5, null_mut());
                        if clipped {
                            let region = CreateRoundRectRgn(21, 23, 213, 237, 19, 21);
                            assert_ne!(SelectClipRgn(target.dc, region), 0);
                            DeleteObject(region);
                            IntersectClipRect(target.dc, 31, 37, 331, 161);
                        }
                    }
                    let rect = Rect {
                        left: origin.0,
                        top: origin.1,
                        right: origin.0 + 257,
                        bottom: origin.1 + 129,
                    };
                    paint_alpha_bitmap(expected.dc, source.handle, &source_image, &rect);
                    flattened_paint(actual.dc, &source, &source_image, rect, cached);
                    assert!(
                        pixels(&actual.bitmap) == pixels(&expected.bitmap),
                        "scale={sx},{sy}, translate={dx},{dy}, origin={origin:?}, clipped={clipped}, cached={cached}"
                    );
                }
            }
        }
    }
}

struct DeviceTarget {
    dc: Hdc,
    bitmap: Hbitmap,
    previous: Hgdiobj,
    width: u32,
    height: u32,
}

impl DeviceTarget {
    unsafe fn new(width: u32, height: u32) -> Self {
        let display = GetDC(null_mut());
        assert!(!display.is_null());
        let dc = CreateCompatibleDC(display);
        let bitmap = CreateCompatibleBitmap(display, width as i32, height as i32);
        ReleaseDC(null_mut(), display);
        assert!(!dc.is_null() && !bitmap.is_null());
        let previous = SelectObject(dc, bitmap);
        assert!(!previous.is_null() && previous as isize != -1);
        crate::windows_app::paint_primitives::fill_color_rect(
            dc,
            &Rect {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
            0x332211,
        );
        Self {
            dc,
            bitmap,
            previous,
            width,
            height,
        }
    }

    unsafe fn pixels(&self) -> Vec<u8> {
        let target = Target::new(&image(self.width, self.height, [0; 4]));
        let saved = SaveDC(self.dc);
        let identity = XFORM {
            eM11: 1.0,
            eM12: 0.0,
            eM21: 0.0,
            eM22: 1.0,
            eDx: 0.0,
            eDy: 0.0,
        };
        assert_ne!(SetWorldTransform(self.dc, &identity), 0);
        SetViewportOrgEx(self.dc, 0, 0, null_mut());
        assert_ne!(
            BitBlt(
                target.dc,
                0,
                0,
                self.width as i32,
                self.height as i32,
                self.dc,
                0,
                0,
                SRCCOPY
            ),
            0
        );
        RestoreDC(self.dc, saved);
        pixels(&target.bitmap)
    }
}

impl Drop for DeviceTarget {
    fn drop(&mut self) {
        unsafe {
            GdiFlush();
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

#[test]
fn device_bitmap_draws_match_with_fractional_transform_and_cached_resampling() {
    unsafe {
        let source_image = patterned(153, 97);
        let source = Bitmap::create(&source_image, &source_image.bgra, null_mut()).unwrap();
        for [sx, sy, dx, dy] in [
            [1.25, 1.25, 13.125, 17.75],
            [1.3, 0.75, -13.375, -7.25],
            [2.125, 1.75, 0.125, 1.875],
            [0.8, 1.2, 21.99, -19.99],
        ] {
            for cached in [false, true] {
                let expected = DeviceTarget::new(640, 400);
                let actual = DeviceTarget::new(640, 400);
                let matrix = XFORM {
                    eM11: sx,
                    eM12: 0.0,
                    eM21: 0.0,
                    eM22: sy,
                    eDx: dx,
                    eDy: dy,
                };
                for target in [&expected, &actual] {
                    assert_ne!(SetGraphicsMode(target.dc, GM_ADVANCED), 0);
                    assert_ne!(SetWorldTransform(target.dc, &matrix), 0);
                    SetViewportOrgEx(target.dc, -3, -5, null_mut());
                    IntersectClipRect(target.dc, 31, 37, 331, 161);
                }
                let rect = Rect {
                    left: -13,
                    top: -17,
                    right: 257 - 13,
                    bottom: 129 - 17,
                };
                paint_alpha_bitmap(expected.dc, source.handle, &source_image, &rect);
                flattened_paint(actual.dc, &source, &source_image, rect, cached);
                assert!(
                    actual.pixels() == expected.pixels(),
                    "device bitmap scale={sx},{sy}, cached={cached}"
                );
            }
        }
    }
}

#[test]
fn scaled_destination_restores_world_matrix_viewport_and_clip_and_declines_other_transforms() {
    unsafe {
        let target = Target::new(&image(640, 400, [0; 4]));
        SetGraphicsMode(target.dc, GM_ADVANCED);
        SetViewportOrgEx(target.dc, -17, 23, null_mut());
        IntersectClipRect(target.dc, 11, 13, 211, 213);
        let rect = Rect {
            left: 31,
            top: 37,
            right: 151,
            bottom: 137,
        };
        for [sx, shear_y, shear_x, sy] in [
            [1.25, 0.0, 0.0, 1.25],
            [1.0, 0.0, 0.0, 1.0],
            [-1.25, 0.0, 0.0, 1.25],
            [1.25, 0.125, 0.0, 1.25],
            [1.25, 0.0, -0.125, 1.25],
            [0.0, 1.0, -1.0, 0.0],
        ] {
            let matrix = XFORM {
                eM11: sx,
                eM12: shear_y,
                eM21: shear_x,
                eM22: sy,
                eDx: 13.5,
                eDy: -7.5,
            };
            assert_ne!(SetWorldTransform(target.dc, &matrix), 0);
            let mut before_clip: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
            GetClipBox(target.dc, &mut before_clip);
            let plan =
                crate::windows_app::painting::affine::ScaledDestination::new(target.dc, rect);
            assert_eq!(
                plan.is_some(),
                sx == 1.25 && shear_y == 0.0 && shear_x == 0.0
            );
            drop(plan);
            let mut after: XFORM = std::mem::zeroed();
            GetWorldTransform(target.dc, &mut after);
            assert_eq!(
                [
                    after.eM11, after.eM12, after.eM21, after.eM22, after.eDx, after.eDy
                ],
                [sx, shear_y, shear_x, sy, 13.5, -7.5]
            );
            let mut viewport = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            GetViewportOrgEx(target.dc, &mut viewport);
            assert_eq!((viewport.x, viewport.y), (-17, 23));
            let mut after_clip: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
            GetClipBox(target.dc, &mut after_clip);
            assert_eq!(
                (
                    after_clip.left,
                    after_clip.top,
                    after_clip.right,
                    after_clip.bottom
                ),
                (
                    before_clip.left,
                    before_clip.top,
                    before_clip.right,
                    before_clip.bottom
                )
            );
        }
    }
}
