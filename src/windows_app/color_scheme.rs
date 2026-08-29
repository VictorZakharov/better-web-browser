//! Windows app color preference exposed to CSS media queries and `matchMedia`.

use super::*;
use std::ffi::c_void;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

pub(super) const WM_SETTINGCHANGE: u32 = 0x001A;
pub(super) const WM_THEMECHANGED: u32 = 0x031A;

pub(super) unsafe fn handle_change(state: &mut BrowserState, window: Hwnd) -> Lresult {
    state
        .app
        .prefers_dark_color_scheme
        .set(prefers_dark_color_scheme());
    state.mark_all_tab_layouts_dirty();
    state.rebuild_layout();
    InvalidateRect(window, null(), 0);
    0
}

pub(super) fn prefers_dark_color_scheme() -> bool {
    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = wide("AppsUseLightTheme");
    let mut value = 1_u32;
    let mut bytes = std::mem::size_of_val(&value) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut value as *mut u32).cast::<c_void>(),
            &mut bytes,
        )
    };
    status == ERROR_SUCCESS && bytes == std::mem::size_of_val(&value) as u32 && value == 0
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
