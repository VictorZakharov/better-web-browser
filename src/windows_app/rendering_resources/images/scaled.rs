//! Cache exact native resampling of repeatedly painted translucent images.
//! AlphaBlend's sampling is not interchangeable with StretchBlt for alpha images.
use super::*;
use std::collections::VecDeque;

const MAX_BYTES: usize = 64 * 1024 * 1024;
// The byte limit remains the primary bound. A 32-entry admission history
// continually forgot images in even a modest 64-sprite repeated frame, so no
// image ever reached its second request and native scaling repeated forever.
const MAX_ENTRIES: usize = 512;
const MAX_PENDING: usize = 1024;
// Data-image URLs can contain megabytes. Optional cache metadata must not copy
// that payload hundreds of times; oversized keys retain direct native painting.
const MAX_SOURCE_KEY_BYTES: usize = 4096;
const MIN_BYTES: usize = 16 * 1024;

pub(in crate::windows_app) struct ScaledImage {
    pub(in crate::windows_app) handle: Hbitmap,
    pub(in crate::windows_app) coverage: Option<std::sync::Arc<[Rect]>>,
}

impl ScaledImage {
    fn from_bitmap(bitmap: &Bitmap) -> Self {
        Self {
            handle: bitmap.handle,
            coverage: bitmap.coverage.as_ref().and_then(|v| v.clone()),
        }
    }
}

#[derive(PartialEq, Eq)]
struct Key {
    source: String,
    width: i32,
    height: i32,
}

impl Key {
    fn matches(&self, source: &str, width: i32, height: i32) -> bool {
        self.source == source && self.width == width && self.height == height
    }
}

struct Variant {
    key: Key,
    bitmap: Bitmap,
}

pub(super) struct Cache {
    entries: VecDeque<Variant>,
    pending: VecDeque<Key>,
    bytes: usize,
    byte_limit: usize,
    entry_limit: usize,
    pending_limit: usize,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            pending: VecDeque::new(),
            bytes: 0,
            byte_limit: MAX_BYTES,
            entry_limit: MAX_ENTRIES,
            pending_limit: MAX_PENDING,
        }
    }
}

impl Cache {
    pub(super) unsafe fn get(
        &mut self,
        key: &str,
        image: &DecodedImage,
        source: Hbitmap,
        destination: &Rect,
        dc: Hdc,
    ) -> Option<ScaledImage> {
        let width = destination.width();
        let height = destination.height();
        if source.is_null()
            || !valid(image)
            || key.len() > MAX_SOURCE_KEY_BYTES
            || !unsafe { local_resampling_matches_device(dc) }
            || width <= 0
            || height <= 0
            || (width == image.width as i32 && height == image.height as i32)
        {
            return None;
        }
        let bytes = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if bytes < MIN_BYTES || bytes > self.byte_limit / 2 {
            return None;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|v| v.key.matches(key, width, height))
        {
            let entry = self.entries.remove(index)?;
            let result = ScaledImage::from_bitmap(&entry.bitmap);
            self.entries.push_back(entry);
            return Some(result);
        }
        // One-shot paints and Canvas surfaces updated every frame should not pay
        // allocation/resampling twice. A second identical request admits a variant.
        let Some(index) = self
            .pending
            .iter()
            .position(|v| v.matches(key, width, height))
        else {
            if self.pending.len() == self.pending_limit {
                self.pending.pop_front();
            }
            self.pending.push_back(Key {
                source: key.into(),
                width,
                height,
            });
            return None;
        };
        let pending = self.pending.remove(index)?;
        while self.bytes.saturating_add(bytes) > self.byte_limit
            || self.entries.len() >= self.entry_limit
        {
            let retired = self.entries.pop_front()?;
            self.bytes -= retired.bitmap.bytes;
        }
        let bitmap = unsafe { resample(source, image, width, height, bytes, dc) }?;
        let result = ScaledImage::from_bitmap(&bitmap);
        self.bytes += bytes;
        self.entries.push_back(Variant {
            key: pending,
            bitmap,
        });
        Some(result)
    }

    pub(super) fn remove(&mut self, source: &str) {
        self.entries.retain(|v| v.key.source != source);
        self.pending.retain(|v| v.source != source);
        self.bytes = self.entries.iter().map(|v| v.bitmap.bytes).sum();
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.pending.clear();
        self.bytes = 0;
    }
}

unsafe fn local_resampling_matches_device(dc: Hdc) -> bool {
    use windows_sys::Win32::Graphics::Gdi::{GetWorldTransform, XFORM};
    if dc.is_null() {
        return true;
    }
    let mut matrix: XFORM = unsafe { std::mem::zeroed() };
    // An affine CSS transform changes the sampling grid. Scaling once locally
    // and again through a destination world transform is not the direct native
    // sampling operation. Decline the optimization, not the image's painting.
    (unsafe { GetWorldTransform(dc, &mut matrix) != 0 })
        && matrix.eM11 == 1.0
        && matrix.eM12 == 0.0
        && matrix.eM21 == 0.0
        && matrix.eM22 == 1.0
}

unsafe fn resample(
    source: Hbitmap,
    image: &DecodedImage,
    width: i32,
    height: i32,
    bytes: usize,
    dc: Hdc,
) -> Option<Bitmap> {
    unsafe {
        let mut info = bitmap_info(image);
        info.header.width = width;
        info.header.height = -height;
        info.header.size_image = bytes as u32;
        let mut bits = null_mut();
        let handle = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if handle.is_null() {
            return None;
        }
        let Some(pixels) = NonNull::new(bits.cast::<u8>()) else {
            DeleteObject(handle);
            return None;
        };
        let mut bitmap = Bitmap {
            handle,
            pixels,
            width: width as u32,
            height: height as u32,
            bytes,
            opaque: false,
            coverage: None,
        };
        // A zero destination preserves all premultiplied source channels while
        // applying exactly the old AlphaBlend sampling, including fractional DPI.
        std::ptr::write_bytes(pixels.as_ptr(), 0, bytes);
        let source_dc = CreateCompatibleDC(dc);
        let target_dc = CreateCompatibleDC(dc);
        if source_dc.is_null() || target_dc.is_null() {
            if !source_dc.is_null() {
                DeleteDC(source_dc);
            }
            if !target_dc.is_null() {
                DeleteDC(target_dc);
            }
            return None;
        }
        let old_source = SelectObject(source_dc, source);
        let old_target = SelectObject(target_dc, handle);
        let selected = |previous: Hgdiobj| !previous.is_null() && previous as isize != -1;
        let ok = selected(old_source)
            && selected(old_target)
            && AlphaBlend(
                target_dc,
                0,
                0,
                width,
                height,
                source_dc,
                0,
                0,
                image.width as i32,
                image.height as i32,
                BlendFunction {
                    operation: 0,
                    flags: 0,
                    source_constant_alpha: 255,
                    alpha_format: 1,
                },
            ) != 0
            && GdiFlush() != 0;
        if selected(old_source) {
            SelectObject(source_dc, old_source);
        }
        if selected(old_target) {
            SelectObject(target_dc, old_target);
        }
        DeleteDC(source_dc);
        DeleteDC(target_dc);
        if !ok {
            return None;
        }
        // Immutable source pixels have just been synchronized. Derive conservative
        // coverage once; drawing must use the resampled extent, not the old mask.
        let pixels = std::slice::from_raw_parts(bitmap.pixels.as_ptr(), bytes);
        bitmap.coverage = Some(super::coverage::derive_bytes(
            width as u32,
            height as u32,
            pixels,
        ));
        Some(bitmap)
    }
}

#[cfg(test)]
mod affine_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod working_set_tests;
