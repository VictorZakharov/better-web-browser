use super::super::tests::{image, pixels, surface::Target};
use super::*;
use crate::windows_app::paint_primitives::{
    paint_alpha_bitmap, paint_alpha_bitmap_size, paint_sparse_bitmap,
};
use windows_sys::Win32::Graphics::Gdi::SelectClipRgn;

fn patterned(width: u32, height: u32) -> DecodedImage {
    let mut result = image(width, height, [0; 4]);
    result.bgra = (0..width * height)
        .flat_map(|index| {
            let alpha = (index % 256) as u8;
            [
                (index % 251 * u32::from(alpha) / 255) as u8,
                (index / 47 % 253 * u32::from(alpha) / 255) as u8,
                (index / 313 % 255 * u32::from(alpha) / 255) as u8,
                alpha,
            ]
        })
        .collect::<Vec<_>>()
        .into();
    result
}

#[test]
fn cached_native_scaling_preserves_every_channel_and_complex_clip_phase() {
    unsafe {
        for (sw, sh, width, height) in [
            (153, 65, 192, 83),
            (120, 80, 288, 123),
            (741, 453, 740, 453),
            (173, 97, 119, 151),
        ] {
            let source = patterned(sw, sh);
            let bitmap = Bitmap::create(&source, &source.bgra, null_mut()).unwrap();
            let mut cache = Cache::default();
            for origin in [(0, 0), (13, 17), (-13, -17)] {
                for clipped in [false, true] {
                    let original = Target::new(&image(800, 550, [77; 4]));
                    let candidate = Target::new(&image(800, 550, [77; 4]));
                    let destination = Rect {
                        left: origin.0,
                        top: origin.1,
                        right: origin.0 + width,
                        bottom: origin.1 + height,
                    };
                    for target in [&original, &candidate] {
                        SetViewportOrgEx(target.dc, -3, -5, null_mut());
                        if clipped {
                            let region = CreateRoundRectRgn(21, 23, 713, 437, 19, 21);
                            assert!(!region.is_null());
                            assert_ne!(SelectClipRgn(target.dc, region), 0);
                            DeleteObject(region);
                            IntersectClipRect(target.dc, 31, 37, 731, 461);
                        }
                    }
                    let cached = cache
                        .get("source", &source, bitmap.handle, &destination, candidate.dc)
                        .or_else(|| {
                            cache.get("source", &source, bitmap.handle, &destination, candidate.dc)
                        })
                        .unwrap();
                    paint_alpha_bitmap(original.dc, bitmap.handle, &source, &destination);
                    paint_alpha_bitmap_size(
                        candidate.dc,
                        cached.handle,
                        width as u32,
                        height as u32,
                        &destination,
                    );
                    assert_eq!(
                        pixels(&original.bitmap),
                        pixels(&candidate.bitmap),
                        "source={sw}x{sh}, destination={width}x{height}, origin={origin:?}, clip={clipped}"
                    );
                }
            }
            assert_eq!(cache.entries.len(), 1);
        }
    }
}

#[test]
fn cached_scaled_sparse_borders_preserve_native_pixels_with_clip_and_translation() {
    unsafe {
        let mut source = image(427, 770, [0; 4]);
        let mut bytes = source.bgra.to_vec();
        for y in 0..770usize {
            for x in 0..427usize {
                if !(2..425).contains(&x) || !(2..768).contains(&y) {
                    bytes[(y * 427 + x) * 4..(y * 427 + x + 1) * 4]
                        .copy_from_slice(&[12, 24, 48, 128]);
                }
            }
        }
        source.bgra = bytes.into();
        let bitmap = Bitmap::create(&source, &source.bgra, null_mut()).unwrap();
        for (width, height) in [(427, 769), (426, 770), (534, 963), (213, 385)] {
            let mut cache = Cache::default();
            for clipped in [false, true] {
                let original = Target::new(&image(700, 1050, [77; 4]));
                let candidate = Target::new(&image(700, 1050, [77; 4]));
                for target in [&original, &candidate] {
                    SetViewportOrgEx(target.dc, -20, -30, null_mut());
                    if clipped {
                        let region = CreateRoundRectRgn(25, 35, 550, 980, 19, 21);
                        assert!(!region.is_null());
                        assert_ne!(SelectClipRgn(target.dc, region), 0);
                        DeleteObject(region);
                        IntersectClipRect(target.dc, 31, 37, 650, 1021);
                    }
                }
                let destination = Rect {
                    left: 30,
                    top: 40,
                    right: 30 + width,
                    bottom: 40 + height,
                };
                let cached = cache
                    .get("border", &source, bitmap.handle, &destination, candidate.dc)
                    .or_else(|| {
                        cache.get("border", &source, bitmap.handle, &destination, candidate.dc)
                    })
                    .unwrap();
                paint_alpha_bitmap(original.dc, bitmap.handle, &source, &destination);
                assert_eq!(cached.coverage.is_some(), width >= 400);
                if let Some(coverage) = cached.coverage.as_deref() {
                    assert!(paint_sparse_bitmap(
                        candidate.dc,
                        cached.handle,
                        &destination,
                        coverage
                    ));
                } else {
                    paint_alpha_bitmap_size(
                        candidate.dc,
                        cached.handle,
                        width as u32,
                        height as u32,
                        &destination,
                    );
                }
                assert_eq!(
                    pixels(&original.bitmap),
                    pixels(&candidate.bitmap),
                    "destination={width}x{height}, clip={clipped}"
                );
            }
        }
    }
}

#[test]
fn source_updates_retire_scaled_pixels_and_reset_two_paint_admission() {
    unsafe {
        let mut images = ImageBitmaps::default();
        let first = patterned(120, 80);
        let rect = Rect {
            left: 0,
            top: 0,
            right: 240,
            bottom: 160,
        };
        let source = images.get_or_create("canvas", &first, null_mut());
        assert!(
            images
                .scaled_image("canvas", &first, source, &rect, null_mut())
                .is_none()
        );
        let handle = images
            .scaled_image("canvas", &first, source, &rect, null_mut())
            .unwrap()
            .handle;
        assert_eq!(
            images
                .scaled_image("canvas", &first, source, &rect, null_mut())
                .map(|v| v.handle),
            Some(handle)
        );
        let old = pixels(&images.scaled.entries[0].bitmap);
        let second = image(120, 80, [0, 64, 0, 128]);
        assert!(images.update("canvas", &second));
        assert_eq!(images.scaled.bytes, 0);
        assert!(
            images
                .scaled_image("canvas", &second, source, &rect, null_mut())
                .is_none()
        );
        images
            .scaled_image("canvas", &second, source, &rect, null_mut())
            .unwrap();
        assert_ne!(pixels(&images.scaled.entries[0].bitmap), old);
        images.remove("canvas");
        assert!(images.scaled.entries.is_empty() && images.scaled.pending.is_empty());
    }
}

#[test]
fn scaled_cache_is_byte_bounded_evicts_oldest_and_leaves_tiny_unscaled_draws_alone() {
    unsafe {
        let source = patterned(120, 80);
        let bitmap = Bitmap::create(&source, &source.bgra, null_mut()).unwrap();
        let mut cache = Cache {
            byte_limit: 384 * 1024,
            ..Cache::default()
        };
        let rect = Rect {
            left: 0,
            top: 0,
            right: 240,
            bottom: 160,
        };
        for name in ["a", "b", "a", "c"] {
            let _ = cache.get(name, &source, bitmap.handle, &rect, null_mut());
            cache
                .get(name, &source, bitmap.handle, &rect, null_mut())
                .unwrap();
            assert!(cache.bytes <= cache.byte_limit);
        }
        assert_eq!(
            cache
                .entries
                .iter()
                .map(|v| v.key.source.as_str())
                .collect::<Vec<_>>(),
            ["a", "c"]
        );
        for extent in [(120, 80), (2, 2), (0, 80), (10000, 10000)] {
            let bounds = Rect {
                right: extent.0,
                bottom: extent.1,
                ..Rect::default()
            };
            assert!(
                cache
                    .get("rejected", &source, bitmap.handle, &bounds, null_mut())
                    .is_none()
            );
        }
        cache.clear();
        assert!(cache.entries.is_empty() && cache.pending.is_empty());
        assert_eq!(cache.bytes, 0);
    }
}
