//! Reject only rectangles proven invisible by the actual native DC clip.
//!
//! Logical viewport bounds alone miss overflow/rounded clips and dirty-region
//! backbuffers. RectVisible applies the DC mapping and selected clip itself;
//! keeping the original draw rectangle avoids changing resampling at its edges.
use super::super::platform::{Hdc, Rect};
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::RectVisible;

pub(super) unsafe fn drawable(dc: Hdc, rect: Rect) -> bool {
    if rect.width() <= 0 || rect.height() <= 0 {
        return false;
    }
    let native = RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    };
    // https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-rectvisible
    unsafe { RectVisible(dc, &native) != 0 }
}

#[cfg(test)]
mod tests;
