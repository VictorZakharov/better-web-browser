//! UI-thread-owned DIBs; same-size pixel updates reuse their native allocation.
use super::*;
use crate::windows_app::paint_primitives::bitmap_info;
use std::ptr::NonNull;
use windows_sys::Win32::Graphics::Gdi::GdiFlush;
mod coverage;
mod opacity;
mod scaled;

struct Bitmap {
    handle: Hbitmap,
    pixels: NonNull<u8>,
    width: u32,
    height: u32,
    bytes: usize,
    opaque: bool,
    coverage: Option<Option<std::sync::Arc<[Rect]>>>,
}

fn valid(image: &DecodedImage) -> bool {
    image.width > 0
        && image.height > 0
        && image.width <= i32::MAX as u32
        && image.height <= i32::MAX as u32
        && (image.width as usize)
            .checked_mul(image.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            == Some(image.bgra.len())
}

impl Bitmap {
    unsafe fn create(image: &DecodedImage, bytes: &[u8], dc: Hdc) -> Option<Self> {
        if !valid(image) || bytes.len() != image.bgra.len() {
            return None;
        }
        let mut pixels = null_mut();
        let handle = CreateDIBSection(
            dc,
            &bitmap_info(image),
            DIB_RGB_COLORS,
            &mut pixels,
            null_mut(),
            0,
        );
        if handle.is_null() {
            return None;
        }
        let Some(pixels) = NonNull::new(pixels.cast()) else {
            DeleteObject(handle);
            return None;
        };
        let bitmap = Self {
            handle,
            pixels,
            width: image.width,
            height: image.height,
            bytes: bytes.len(),
            opaque: opacity::all_opaque(bytes),
            coverage: None,
        };
        // SAFETY: exact-sized new DIB, no pending drawing, disjoint immutable source.
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), bitmap.pixels.as_ptr(), bytes.len());
        Some(bitmap)
    }

    unsafe fn refresh(&mut self, image: &DecodedImage) -> bool {
        if !valid(image) || self.width != image.width || self.height != image.height {
            return false;
        }
        // Painting restores selected source bitmaps before an update can run.
        // Complete this thread's earlier drawing before CPU writes to DIB memory.
        // https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-createdibsection
        if GdiFlush() == 0 {
            return false;
        }
        // SAFETY: this UI thread uniquely owns the live, dimension-checked DIB;
        // renderer BGRA bytes are independent immutable Arc storage.
        std::ptr::copy_nonoverlapping(image.bgra.as_ptr(), self.pixels.as_ptr(), self.bytes);
        self.opaque = opacity::all_opaque(&image.bgra);
        self.coverage = None;
        true
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        // No cached bitmap remains selected. Flush pending drawing before storage retirement.
        unsafe {
            GdiFlush();
            DeleteObject(self.handle);
        }
    }
}

#[derive(Default)]
pub(in crate::windows_app) struct ImageBitmaps {
    bitmaps: HashMap<String, Bitmap>,
    scaled: scaled::Cache,
}

impl ImageBitmaps {
    pub(in crate::windows_app) unsafe fn scaled_image(
        &mut self,
        key: &str,
        image: &DecodedImage,
        bitmap: Hbitmap,
        destination: &Rect,
        dc: Hdc,
    ) -> Option<scaled::ScaledImage> {
        unsafe { self.scaled.get(key, image, bitmap, destination, dc) }
    }

    pub(in crate::windows_app) fn is_opaque(&self, key: &str) -> bool {
        self.bitmaps.get(key).is_some_and(|bitmap| bitmap.opaque)
    }

    pub(in crate::windows_app) fn sparse_coverage(
        &mut self,
        key: &str,
        image: &DecodedImage,
        destination: &Rect,
    ) -> Option<std::sync::Arc<[Rect]>> {
        let bitmap = self.bitmaps.get_mut(key)?;
        if bitmap.opaque
            || destination.width() != image.width as i32
            || destination.height() != image.height as i32
        {
            return None;
        }
        bitmap
            .coverage
            .get_or_insert_with(|| coverage::derive(image))
            .clone()
    }

    pub(in crate::windows_app) unsafe fn get_or_create(
        &mut self,
        key: &str,
        image: &DecodedImage,
        dc: Hdc,
    ) -> Hbitmap {
        if let Some(bitmap) = self.bitmaps.get(key) {
            return bitmap.handle;
        }
        let Some(bitmap) = Bitmap::create(image, &image.bgra, dc) else {
            return null_mut();
        };
        let handle = bitmap.handle;
        self.bitmaps.insert(key.to_string(), bitmap);
        handle
    }

    pub(in crate::windows_app) unsafe fn update(
        &mut self,
        key: &str,
        image: &DecodedImage,
    ) -> bool {
        self.remove_tints(key);
        self.scaled.remove(key);
        if let Some(bitmap) = self.bitmaps.get_mut(key) {
            if bitmap.refresh(image) {
                return true;
            }
            // Resizing, malformed input or synchronization failure cannot reuse stale pixels.
            self.bitmaps.remove(key);
        }
        false
    }

    pub(in crate::windows_app) unsafe fn get_or_create_tinted(
        &mut self,
        key: &str,
        image: &DecodedImage,
        tint: [u8; 4],
        dc: Hdc,
    ) -> Hbitmap {
        let cache_key = format!(
            "{key}#tint:{:02x}{:02x}{:02x}{:02x}",
            tint[0], tint[1], tint[2], tint[3]
        );
        if let Some(bitmap) = self.bitmaps.get(&cache_key) {
            return bitmap.handle;
        }
        if !valid(image) {
            return null_mut();
        }
        let mut tinted = Vec::with_capacity(image.bgra.len());
        for pixel in image.bgra.chunks_exact(4) {
            let alpha = u16::from(pixel[3]) * u16::from(tint[3]) / 255;
            tinted.extend([
                (u16::from(tint[2]) * alpha / 255) as u8,
                (u16::from(tint[1]) * alpha / 255) as u8,
                (u16::from(tint[0]) * alpha / 255) as u8,
                alpha as u8,
            ]);
        }
        let Some(bitmap) = Bitmap::create(image, &tinted, dc) else {
            return null_mut();
        };
        let handle = bitmap.handle;
        self.bitmaps.insert(cache_key, bitmap);
        handle
    }

    pub(in crate::windows_app) unsafe fn clear(&mut self) {
        self.scaled.clear();
        self.bitmaps.clear();
    }

    unsafe fn remove_tints(&mut self, key: &str) {
        let prefix = format!("{key}#tint:");
        self.bitmaps
            .retain(|candidate, _| !candidate.starts_with(&prefix));
    }

    pub(in crate::windows_app) unsafe fn remove(&mut self, key: &str) {
        self.scaled.remove(key);
        self.remove_tints(key);
        self.bitmaps.remove(key);
    }
}

#[cfg(test)]
mod tests;
