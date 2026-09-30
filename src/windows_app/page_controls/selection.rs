//! UTF-16 selection coordinates between Win32 EDIT and HTML text controls.

use super::*;

pub(in crate::windows_app) const WM_APP_PAGE_CONTROL_SELECTION: u32 = WM_APP + 23;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const MK_LBUTTON: usize = 0x0001;

pub(in crate::windows_app) fn is_text_edit(kind: ControlKind) -> bool {
    matches!(
        kind,
        ControlKind::Text | ControlKind::TextArea | ControlKind::Password | ControlKind::Search
    )
}

pub(in crate::windows_app) fn selection_may_change(message: u32, wparam: Wparam) -> bool {
    matches!(
        message,
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_SETFOCUS
    ) || message == WM_MOUSEMOVE && wparam & MK_LBUTTON != 0
}

pub(in crate::windows_app) unsafe fn read_edit_selection(window: Hwnd) -> (u32, u32) {
    let mut start = 0_u32;
    let mut end = 0_u32;
    SendMessageW(
        window,
        EM_GETSEL,
        (&mut start as *mut u32) as usize,
        (&mut end as *mut u32) as isize,
    );
    (start, end)
}

pub(in crate::windows_app) unsafe fn edit_text_and_selection(
    window: Hwnd,
    kind: ControlKind,
) -> (String, u32, u32) {
    let native = window_text(window);
    let (start, end) = read_edit_selection(window);
    if kind == ControlKind::TextArea {
        (
            normalize_edit_newlines(&native),
            native_to_html_offset(&native, start),
            native_to_html_offset(&native, end),
        )
    } else {
        (native, start, end)
    }
}

pub(in crate::windows_app) fn normalize_edit_newlines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

pub(in crate::windows_app) fn native_to_html_offset(native: &str, offset: u32) -> u32 {
    let units = native.encode_utf16().collect::<Vec<_>>();
    let mut position = 0;
    let mut html = 0;
    while position < units.len() && position < offset as usize {
        if units[position] == b'\r' as u16 && units.get(position + 1) == Some(&(b'\n' as u16)) {
            position += 2;
        } else {
            position += 1;
        }
        html += 1;
    }
    html
}

pub(in crate::windows_app) fn html_to_native_offset(native: &str, offset: u32) -> u32 {
    let units = native.encode_utf16().collect::<Vec<_>>();
    let mut position = 0;
    let mut html = 0;
    while position < units.len() && html < offset {
        if units[position] == b'\r' as u16 && units.get(position + 1) == Some(&(b'\n' as u16)) {
            position += 2;
        } else {
            position += 1;
        }
        html += 1;
    }
    position.min(u32::MAX as usize) as u32
}

pub(in crate::windows_app) fn native_edit_text(value: &str) -> String {
    normalize_edit_newlines(value).replace('\n', "\r\n")
}

pub(in crate::windows_app) unsafe fn sync_html_value(window: Hwnd, kind: ControlKind, value: &str) {
    let (current, _, _) = edit_text_and_selection(window, kind);
    if current != value {
        let native = if kind == ControlKind::TextArea {
            native_edit_text(value)
        } else {
            value.to_owned()
        };
        set_window_text(window, &native);
    }
}

pub(in crate::windows_app) unsafe fn apply_html_selection(
    window: Hwnd,
    kind: ControlKind,
    start: u32,
    end: u32,
    backward: bool,
) -> (u32, u32) {
    let value = window_text(window);
    let to_native = |position| {
        if kind == ControlKind::TextArea {
            html_to_native_offset(&value, position)
        } else {
            position.min(value.encode_utf16().count().min(u32::MAX as usize) as u32)
        }
    };
    let start = to_native(start);
    let end = to_native(end);
    if backward {
        SendMessageW(window, EM_SETSEL, end as usize, start as isize);
    } else {
        SendMessageW(window, EM_SETSEL, start as usize, end as isize);
    }
    let (_, applied_start, applied_end) = edit_text_and_selection(window, kind);
    (applied_start, applied_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiline_offsets_collapse_crlf_but_count_utf16_surrogates() {
        let native = "A\r\n😀\r\nZ";
        let html = normalize_edit_newlines(native);
        assert_eq!(html, "A\n😀\nZ");
        for (html_offset, native_offset) in [(0, 0), (1, 1), (2, 3), (3, 4), (4, 5), (5, 7), (6, 8)]
        {
            assert_eq!(html_to_native_offset(native, html_offset), native_offset);
            assert_eq!(native_to_html_offset(native, native_offset), html_offset);
        }
        assert_eq!(native_to_html_offset(native, 2), 2);
        assert_eq!(html_to_native_offset(native, 999), 8);
    }

    #[test]
    fn lone_cr_and_lf_each_keep_one_selection_position() {
        let native = "a\rb\nc";
        assert_eq!(normalize_edit_newlines(native), "a\nb\nc");
        for offset in 0..=5 {
            assert_eq!(html_to_native_offset(native, offset), offset);
            assert_eq!(native_to_html_offset(native, offset), offset);
        }
    }

    #[test]
    fn textarea_text_roundtrips_through_win32_line_endings() {
        assert_eq!(native_edit_text("a\nb"), "a\r\nb");
        assert_eq!(native_edit_text("a\r\nb"), "a\r\nb");
    }

    #[test]
    fn only_user_selection_messages_need_a_post_default_check() {
        assert!(selection_may_change(WM_KEYDOWN, 0));
        assert!(selection_may_change(WM_MOUSEMOVE, MK_LBUTTON));
        assert!(!selection_may_change(WM_MOUSEMOVE, 0));
        assert!(!selection_may_change(EM_SETSEL, 0));
    }

    #[test]
    fn scripted_value_and_range_update_the_same_hidden_native_edit() {
        unsafe {
            // No WS_VISIBLE: this native-control check cannot open a visible window.
            let window = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                wide("old").as_ptr(),
                WS_POPUP | ES_MULTILINE,
                0,
                0,
                100,
                50,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            );
            assert!(!window.is_null());
            sync_html_value(window, ControlKind::TextArea, "A\n😀Z");
            assert_eq!(
                apply_html_selection(window, ControlKind::TextArea, 2, 4, true),
                (2, 4)
            );
            assert_eq!(
                edit_text_and_selection(window, ControlKind::TextArea),
                ("A\n😀Z".into(), 2, 4)
            );
            assert_ne!(DestroyWindow(window), 0);
        }
    }
}
