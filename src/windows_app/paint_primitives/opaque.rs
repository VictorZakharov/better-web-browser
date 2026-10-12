//! Verified opaque BGRA images need a copy, not per-pixel source-over blending.
use super::*;
use windows_sys::Win32::Graphics::Gdi::{COLORONCOLOR, SetStretchBltMode, StretchBlt};

pub(super) unsafe fn paint_from_dc(
    destination: Hdc,
    source: Hdc,
    bitmap: Hbitmap,
    image: &DecodedImage,
    rect: &Rect,
) -> bool {
    let owned_source = source.is_null();
    let source = if owned_source {
        CreateCompatibleDC(destination)
    } else {
        source
    };
    if source.is_null() {
        return false;
    }
    let previous_bitmap = SelectObject(source, bitmap);
    let old_mode = SetStretchBltMode(destination, COLORONCOLOR);
    // AlphaBlend forces COLORONCOLOR regardless of the DC mode. Preserve its
    // sampling and the caller's DC state; failed copies fall back to AlphaBlend.
    // https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-alphablend
    let valid_selection = !previous_bitmap.is_null() && previous_bitmap as usize != usize::MAX;
    let copied = if old_mode != 0 && valid_selection {
        StretchBlt(
            destination,
            rect.left,
            rect.top,
            rect.width().max(1),
            rect.height().max(1),
            source,
            0,
            0,
            image.width as i32,
            image.height as i32,
            SRCCOPY,
        ) != 0
    } else {
        false
    };
    if old_mode != 0 {
        SetStretchBltMode(destination, old_mode);
    }
    if valid_selection {
        SelectObject(source, previous_bitmap);
    }
    if owned_source {
        DeleteDC(source);
    }
    copied
}
