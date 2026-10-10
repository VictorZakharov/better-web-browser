use super::*;
use crate::windows_app::paint_primitives::paint_alpha_bitmap;
mod sparse;
pub(in crate::windows_app::rendering_resources::images) mod surface;

pub(in crate::windows_app::rendering_resources::images) fn image(
    width: u32,
    height: u32,
    pixel: [u8; 4],
) -> DecodedImage {
    DecodedImage {
        width,
        height,
        bgra: pixel.repeat(width as usize * height as usize).into(),
    }
}

pub(in crate::windows_app::rendering_resources::images) fn pixels(bitmap: &Bitmap) -> Vec<u8> {
    // SAFETY: tests own the complete live DIB on this thread; finish drawing before reading.
    unsafe {
        assert_ne!(GdiFlush(), 0);
        std::slice::from_raw_parts(bitmap.pixels.as_ptr(), bitmap.bytes).to_vec()
    }
}

#[test]
fn same_size_refresh_keeps_the_native_handle_and_updates_all_owned_bytes() {
    unsafe {
        let mut cache = ImageBitmaps::default();
        let old = image(4, 3, [0, 0, 255, 255]);
        let red = cache.get_or_create("canvas", &old, null_mut());
        let peer = cache.get_or_create("peer", &old, null_mut());
        assert!(!red.is_null() && !peer.is_null());
        let new = image(4, 3, [0, 255, 0, 255]);
        assert!(cache.update("canvas", &new));
        assert_eq!(cache.get_or_create("canvas", &new, null_mut()), red);
        assert_eq!(pixels(&cache.bitmaps["canvas"]), new.bgra.as_ref());
        assert_eq!(pixels(&cache.bitmaps["peer"]), old.bgra.as_ref());
        assert_eq!(old.bgra.as_ref(), [0, 0, 255, 255].repeat(12));
        cache.remove("canvas");
        assert!(!cache.bitmaps.contains_key("canvas"));
        assert!(cache.bitmaps.contains_key("peer"));
        cache.clear();
        assert!(cache.bitmaps.is_empty());
    }
}

#[test]
fn refresh_invalidates_derived_tints_and_resize_replaces_the_extent() {
    unsafe {
        let mut cache = ImageBitmaps::default();
        let old = image(2, 1, [0, 0, 128, 128]);
        cache.get_or_create("canvas", &old, null_mut());
        let tint = [100, 80, 50, 128];
        cache.get_or_create_tinted("canvas", &old, tint, null_mut());
        assert_eq!(
            pixels(&cache.bitmaps["canvas#tint:64503280"]),
            [12, 20, 25, 64].repeat(2)
        );
        let new = image(2, 1, [0, 255, 0, 255]);
        assert!(cache.update("canvas", &new));
        assert_eq!(cache.bitmaps.len(), 1);
        cache.get_or_create_tinted("canvas", &new, tint, null_mut());
        assert_eq!(
            pixels(&cache.bitmaps["canvas#tint:64503280"]),
            [25, 40, 50, 128].repeat(2)
        );
        let resized = image(1, 2, [255, 0, 0, 255]);
        assert!(!cache.update("canvas", &resized));
        assert!(cache.bitmaps.is_empty());
        cache.get_or_create("canvas", &resized, null_mut());
        let bitmap = &cache.bitmaps["canvas"];
        assert_eq!((bitmap.width, bitmap.height), (1, 2));
        assert_eq!(pixels(bitmap), resized.bgra.as_ref());
    }
}

#[test]
fn opacity_is_recomputed_after_refresh_and_never_guessed_from_resource_key() {
    unsafe {
        let mut cache = ImageBitmaps::default();
        let opaque = image(5, 3, [0, 0, 255, 255]);
        cache.get_or_create("canvas", &opaque, null_mut());
        assert!(cache.is_opaque("canvas"));
        assert!(!cache.is_opaque("unknown"));
        let translucent = image(5, 3, [0, 0, 128, 128]);
        assert!(cache.update("canvas", &translucent));
        assert!(!cache.is_opaque("canvas"));
        assert!(cache.update("canvas", &opaque));
        assert!(cache.is_opaque("canvas"));
        cache.get_or_create_tinted("canvas", &opaque, [0, 255, 0, 128], null_mut());
        assert!(!cache.is_opaque("canvas#tint:00ff0080"));
        cache.clear();
    }
}

#[test]
fn malformed_or_zero_extent_cannot_overwrite_or_retain_an_old_bitmap() {
    unsafe {
        let mut cache = ImageBitmaps::default();
        let valid_image = image(1, 1, [255; 4]);
        for invalid in [
            DecodedImage {
                width: 0,
                height: 1,
                bgra: vec![255; 4].into(),
            },
            DecodedImage {
                width: 1,
                height: 1,
                bgra: vec![255; 3].into(),
            },
            DecodedImage {
                width: u32::MAX,
                height: 1,
                bgra: vec![255; 4].into(),
            },
        ] {
            assert!(
                !cache
                    .get_or_create("canvas", &valid_image, null_mut())
                    .is_null()
            );
            assert!(!cache.update("canvas", &invalid));
            assert!(
                cache
                    .get_or_create("canvas", &invalid, null_mut())
                    .is_null()
            );
            assert!(cache.bitmaps.is_empty());
        }
    }
}

struct Selection {
    dc: Hdc,
    previous: Hbitmap,
}
impl Selection {
    unsafe fn new(bitmap: Hbitmap) -> Self {
        let dc = CreateCompatibleDC(null_mut());
        assert!(!dc.is_null());
        let previous = SelectObject(dc, bitmap);
        assert!(!previous.is_null());
        Self { dc, previous }
    }
}
impl Drop for Selection {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteDC(self.dc);
        }
    }
}

#[test]
fn refresh_finishes_prior_gdi_drawing_before_changing_the_source_storage() {
    unsafe {
        let mut cache = ImageBitmaps::default();
        let red = image(1, 1, [0, 0, 255, 255]);
        let green = image(1, 1, [0, 255, 0, 255]);
        let handle = cache.get_or_create("canvas", &red, null_mut());
        let target = Bitmap::create(&green, &green.bgra, null_mut()).unwrap();
        let selected = Selection::new(target.handle);
        let rect = Rect {
            left: 0,
            top: 0,
            right: 1,
            bottom: 1,
        };
        paint_alpha_bitmap(selected.dc, handle, &red, &rect);
        assert!(cache.update("canvas", &green));
        assert_eq!(
            pixels(&target),
            red.bgra.as_ref(),
            "earlier draw must see earlier bytes"
        );
        paint_alpha_bitmap(selected.dc, handle, &green, &rect);
        assert_eq!(
            pixels(&target),
            green.bgra.as_ref(),
            "next draw sees refreshed bytes"
        );
    }
}
