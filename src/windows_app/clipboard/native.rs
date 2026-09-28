//! Narrow CF_UNICODETEXT adapter shared by page Clipboard and diagnostics.
//!
//! Win32 requires movable global memory and transfers ownership only after a
//! successful SetClipboardData. Allocate before EmptyClipboard so OOM does not
//! erase the previous clipboard contents.
use super::*;
use crate::windows_app::platform::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GlobalAlloc, GlobalFree, GlobalLock,
    GlobalSize, GlobalUnlock, IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
};
use better_web_browser::renderer_protocol::MAX_CLIPBOARD_TEXT_BYTES;
use std::ptr::null_mut;

const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x0002;
const MAX_UTF16_BYTES: usize = MAX_CLIPBOARD_TEXT_BYTES * 2 + 2;

pub(super) struct NativeClipboard;

impl ClipboardBackend for NativeClipboard {
    fn read_text(&self, owner: Hwnd) -> Result<String, ClipboardError> {
        read_unicode_text(owner)
    }

    fn write_text(&self, owner: Hwnd, text: &str) -> Result<(), ClipboardError> {
        write_unicode_text(owner, text)
    }
}

struct OpenedClipboard;
impl OpenedClipboard {
    unsafe fn open(owner: Hwnd) -> Result<Self, ClipboardError> {
        if OpenClipboard(owner) == 0 {
            Err(ClipboardError::NotReadable)
        } else {
            Ok(Self)
        }
    }
}
impl Drop for OpenedClipboard {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

struct OwnedGlobal(crate::windows_app::platform::Handle);
impl Drop for OwnedGlobal {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { GlobalFree(self.0) };
        }
    }
}

pub(in crate::windows_app) fn read_unicode_text(owner: Hwnd) -> Result<String, ClipboardError> {
    unsafe {
        let _open = OpenedClipboard::open(owner)?;
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return Err(ClipboardError::NotFound);
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            return Err(ClipboardError::NotReadable);
        }
        let size = GlobalSize(handle);
        let units_to_scan = bounded_unit_count(size)?;
        let pointer = GlobalLock(handle) as *const u16;
        if pointer.is_null() {
            return Err(ClipboardError::NotReadable);
        }
        let units = std::slice::from_raw_parts(pointer, units_to_scan);
        let text = decode_unicode_units(units);
        GlobalUnlock(handle);
        text
    }
}

fn bounded_unit_count(global_size: usize) -> Result<usize, ClipboardError> {
    let count = global_size / 2;
    if count == 0 {
        return Err(ClipboardError::NotReadable);
    }
    // GlobalSize reports the allocated block size, which Windows may round up.
    // Only the NUL-terminated text prefix is subject to our content limit.
    Ok(count.min(MAX_UTF16_BYTES / 2))
}

fn decode_unicode_units(units: &[u16]) -> Result<String, ClipboardError> {
    let end = units
        .iter()
        .position(|unit| *unit == 0)
        .ok_or(ClipboardError::NotReadable)?;
    let text = String::from_utf16_lossy(&units[..end]);
    if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
        return Err(ClipboardError::NotReadable);
    }
    Ok(text)
}

fn windows_newlines(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut previous = '\0';
    for character in text.chars() {
        if character == '\n' && previous != '\r' {
            normalized.push('\r');
        }
        normalized.push(character);
        previous = character;
    }
    normalized
}

pub(in crate::windows_app) fn write_unicode_text(
    owner: Hwnd,
    text: &str,
) -> Result<(), ClipboardError> {
    if text.len() > MAX_CLIPBOARD_TEXT_BYTES || text.contains('\0') {
        return Err(ClipboardError::NotReadable);
    }
    let normalized = windows_newlines(text);
    let mut units: Vec<u16> = normalized.encode_utf16().collect();
    units.push(0);
    let bytes = units
        .len()
        .checked_mul(2)
        .ok_or(ClipboardError::NotReadable)?;
    if bytes > MAX_UTF16_BYTES {
        return Err(ClipboardError::NotReadable);
    }
    unsafe {
        // Prepare the handle while the user's old clipboard is still intact.
        let mut memory = OwnedGlobal(GlobalAlloc(GMEM_MOVEABLE, bytes));
        if memory.0.is_null() {
            return Err(ClipboardError::NotReadable);
        }
        let destination = GlobalLock(memory.0) as *mut u16;
        if destination.is_null() {
            return Err(ClipboardError::NotReadable);
        }
        std::ptr::copy_nonoverlapping(units.as_ptr(), destination, units.len());
        GlobalUnlock(memory.0);
        let _open = OpenedClipboard::open(owner)?;
        if EmptyClipboard() == 0 || SetClipboardData(CF_UNICODETEXT, memory.0).is_null() {
            return Err(ClipboardError::NotReadable);
        }
        // Win32 now owns the global memory.
        memory.0 = null_mut();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_global_allocation_does_not_reject_short_text() {
        let mut units = vec![0u16; MAX_UTF16_BYTES / 2 + 2048];
        units[..3].copy_from_slice(&[u16::from(b'o'), u16::from(b'k'), 0]);
        let count = bounded_unit_count(units.len() * 2).unwrap();
        assert_eq!(count, MAX_UTF16_BYTES / 2);
        assert_eq!(decode_unicode_units(&units[..count]).unwrap(), "ok");
    }

    #[test]
    fn unterminated_or_oversized_unicode_text_is_rejected() {
        assert!(decode_unicode_units(&[u16::from(b'a')]).is_err());
        let mut units = vec![u16::from(b'a'); MAX_CLIPBOARD_TEXT_BYTES + 1];
        units[MAX_CLIPBOARD_TEXT_BYTES] = 0;
        assert!(decode_unicode_units(&units).is_ok());
        units[MAX_CLIPBOARD_TEXT_BYTES] = u16::from(b'a');
        units.push(0);
        assert!(decode_unicode_units(&units).is_err());
    }

    #[test]
    fn windows_text_normalizes_only_lone_line_feeds() {
        assert_eq!(windows_newlines("a\nb\r\nc"), "a\r\nb\r\nc");
    }
}
