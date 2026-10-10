use super::surface::Target;
use super::*;
use crate::windows_app::paint_primitives::paint_sparse_bitmap;
use windows_sys::Win32::Graphics::Gdi::SelectClipRgn;
fn border() -> DecodedImage {
    let mut source = image(427, 770, [0; 4]);
    let mut pixels = source.bgra.to_vec();
    for y in 0..770usize {
        for x in 0..427usize {
            if !(2..425).contains(&x) || !(2..768).contains(&y) {
                pixels[(y * 427 + x) * 4..(y * 427 + x) * 4 + 4]
                    .copy_from_slice(&[12, 24, 48, 128]);
            }
        }
    }
    source.bgra = pixels.into();
    source
}
#[test]
fn sparse_native_blend_preserves_every_channel_with_translation_and_complex_clipping() {
    unsafe {
        let source = border();
        let bitmap = Bitmap::create(&source, &source.bgra, null_mut()).unwrap();
        let cover = coverage::derive(&source).unwrap();
        for clipped in [false, true] {
            let old = Target::new(&image(500, 900, [77; 4]));
            let new = Target::new(&image(500, 900, [77; 4]));
            for target in [&old, &new] {
                SetViewportOrgEx(target.dc, -20, -30, null_mut());
                if clipped {
                    let region = CreateRoundRectRgn(25, 35, 400, 780, 19, 21);
                    assert!(!region.is_null());
                    SelectClipRgn(target.dc, region);
                    DeleteObject(region);
                    IntersectClipRect(target.dc, 31, 37, 450, 821);
                }
            }
            let bounds = Rect {
                left: 30,
                top: 40,
                right: 457,
                bottom: 810,
            };
            paint_alpha_bitmap(old.dc, bitmap.handle, &source, &bounds);
            assert!(paint_sparse_bitmap(new.dc, bitmap.handle, &bounds, &cover));
            assert_eq!(pixels(&old.bitmap), pixels(&new.bitmap), "clip={clipped}");
        }
    }
}

#[test]
fn coverage_is_cached_only_for_unscaled_draws_and_rebuilt_after_pixel_updates() {
    unsafe {
        let mut cache = ImageBitmaps::default();
        let source = border();
        cache.get_or_create("border", &source, null_mut());
        let bounds = Rect {
            left: 0,
            top: 0,
            right: 427,
            bottom: 770,
        };
        let first = cache.sparse_coverage("border", &source, &bounds).unwrap();
        let second = cache.sparse_coverage("border", &source, &bounds).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert!(
            cache
                .sparse_coverage(
                    "border",
                    &source,
                    &Rect {
                        right: 428,
                        ..bounds
                    }
                )
                .is_none()
        );
        let dense = image(427, 770, [0, 0, 0, 128]);
        assert!(cache.update("border", &dense));
        assert!(cache.sparse_coverage("border", &dense, &bounds).is_none());
        assert!(cache.update("border", &source));
        let refreshed = cache.sparse_coverage("border", &source, &bounds).unwrap();
        assert!(!std::sync::Arc::ptr_eq(&first, &refreshed));
        cache.clear();
    }
}
