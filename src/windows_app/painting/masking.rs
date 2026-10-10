//! Bounded group masks are composited against the actual destination, not white.
use super::super::paint_primitives::{bitmap_info, screen_rect};
use super::super::platform::*;
use better_web_browser::engine::{DecodedImage, RectF};
use std::ptr::null_mut;
mod pixels;
mod sampling;

pub(super) struct MaskLayer {
    destination: Hdc,
    source: Hdc,
    bitmap: Hbitmap,
    previous: Hgdiobj,
    pixels: *mut u8,
    rect: Rect,
    full: Rect,
    mask: DecodedImage,
}

impl MaskLayer {
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn begin(
        destination: Hdc,
        bounds: RectF,
        mask: &DecodedImage,
        scroll_y: i32,
        toolbar: i32,
        scale: f32,
        content: &Rect,
        dirty: &Rect,
    ) -> Option<Self> {
        let full = screen_rect(bounds, scroll_y, toolbar, scale);
        let rect = super::opacity::intersection(full, *content)
            .and_then(|r| super::opacity::intersection(r, *dirty))?;
        let bytes = rect.width() as usize * rect.height() as usize * 4;
        if bytes > 64 * 1024 * 1024
            || mask.width == 0
            || mask.height == 0
            || mask.bgra.len() != mask.width as usize * mask.height as usize * 4
        {
            return None;
        }
        let source = CreateCompatibleDC(destination);
        if source.is_null() {
            return None;
        }
        let mut info = bitmap_info(mask);
        info.header.width = rect.width();
        info.header.height = -rect.height();
        info.header.size_image = bytes as u32;
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(destination, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            DeleteDC(source);
            return None;
        }
        let previous = SelectObject(source, bitmap);
        SetViewportOrgEx(source, -rect.left, -rect.top, null_mut());
        IntersectClipRect(source, rect.left, rect.top, rect.right, rect.bottom);
        if BitBlt(
            source,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            destination,
            rect.left,
            rect.top,
            SRCCOPY,
        ) == 0
        {
            SelectObject(source, previous);
            DeleteObject(bitmap);
            DeleteDC(source);
            return None;
        }
        Some(Self {
            destination,
            source,
            bitmap,
            previous,
            pixels: bits.cast(),
            rect,
            full,
            mask: mask.clone(),
        })
    }

    pub(super) fn dc(&self) -> Hdc {
        self.source
    }

    pub(super) unsafe fn finish(self) {
        // GDI batches commands. Synchronize before accessing DIB bytes on the CPU.
        if GdiFlush() != 0 {
            let pixels = std::slice::from_raw_parts_mut(
                self.pixels,
                self.rect.width() as usize * self.rect.height() as usize * 4,
            );
            apply_mask(pixels, self.rect, self.full, &self.mask);
            AlphaBlend(
                self.destination,
                self.rect.left,
                self.rect.top,
                self.rect.width(),
                self.rect.height(),
                self.source,
                self.rect.left,
                self.rect.top,
                self.rect.width(),
                self.rect.height(),
                BlendFunction {
                    operation: 0,
                    flags: 0,
                    source_constant_alpha: 255,
                    alpha_format: 1,
                },
            );
        }
        SelectObject(self.source, self.previous);
        DeleteObject(self.bitmap);
        DeleteDC(self.source);
    }
}

fn apply_mask(pixels: &mut [u8], rect: Rect, full: Rect, mask: &DecodedImage) {
    sampling::apply(
        pixels,
        [rect.left, rect.top, rect.right, rect.bottom],
        [full.left, full.top, full.right, full.bottom],
        [mask.width, mask.height],
        &mask.bgra,
    );
}

#[cfg(test)]
mod tests;
