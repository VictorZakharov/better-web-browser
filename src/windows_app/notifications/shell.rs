//! Win32 notification-area balloon backend, reached only after a browser grant.

use crate::windows_app::platform::Hwnd;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{IDI_APPLICATION, LoadIconW};

pub(in crate::windows_app) const WM_APP_NOTIFICATION: u32 =
    crate::windows_app::platform::WM_APP + 19;

fn wide_into<const N: usize>(text: &str) -> [u16; N] {
    let mut result = [0; N];
    let mut offset = 0;
    for character in text.chars() {
        let mut encoded = [0; 2];
        let units = character.encode_utf16(&mut encoded);
        if offset + units.len() >= N {
            break;
        }
        result[offset..offset + units.len()].copy_from_slice(units);
        offset += units.len();
    }
    result
}

fn identity(window: Hwnd, id: u32) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: id,
        ..Default::default()
    }
}

pub(super) fn show(window: Hwnd, id: u32, title: &str, body: &str) -> bool {
    let mut data = identity(window, id);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_APP_NOTIFICATION;
    data.hIcon = unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) };
    if data.hIcon.is_null() {
        return false;
    }
    data.szTip = wide_into("Breeze");
    if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
        return false;
    }
    data.uFlags |= NIF_INFO;
    data.szInfoTitle = wide_into(if title.is_empty() {
        "Breeze notification"
    } else {
        title
    });
    data.szInfo = wide_into(if body.is_empty() {
        "Notification"
    } else {
        body
    });
    data.dwInfoFlags = NIIF_INFO;
    if unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) } == 0 {
        close(window, id);
        return false;
    }
    true
}

pub(super) fn close(window: Hwnd, id: u32) {
    let data = identity(window, id);
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &data);
    }
}
