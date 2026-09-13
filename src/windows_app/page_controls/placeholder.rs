//! EDIT cue banners ignore authored CSS colors. Paint the placeholder without changing
//! the editable value, selection, or accessibility value of the native control.
use super::*;
use crate::windows_app::paint_primitives::draw_text_in_rect;

struct Placeholder {
    text: String,
    color: u32,
    multiline: bool,
}

pub(super) unsafe fn install(window: Hwnd, spec: &better_web_browser::engine::ControlSpec) {
    let data = Box::into_raw(Box::new(Placeholder {
        text: spec.placeholder.clone(),
        color: spec.placeholder_color.to_colorref(),
        multiline: spec.kind == ControlKind::TextArea,
    }));
    if SetWindowSubclass(window, Some(paint), 2, data as usize) == 0 {
        drop(Box::from_raw(data));
    }
}

unsafe extern "system" fn paint(
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
    subclass_id: usize,
    reference: usize,
) -> Lresult {
    // Win32 EDIT formatting rectangle, font query, and offscreen client paint messages.
    const EM_GETRECT: u32 = 0x00b2;
    const WM_GETFONT: u32 = 0x0031;
    const WM_PRINTCLIENT: u32 = 0x0318;
    const DT_WORDBREAK: u32 = 0x0010;
    if message == WM_NCDESTROY {
        RemoveWindowSubclass(window, Some(paint), subclass_id);
        drop(Box::from_raw(reference as *mut Placeholder));
        return DefSubclassProc(window, message, wparam, lparam);
    }
    let result = DefSubclassProc(window, message, wparam, lparam);
    if matches!(message, WM_PAINT | WM_PRINTCLIENT) && window_text(window).is_empty() {
        let data = &*(reference as *const Placeholder);
        let dc = if message == WM_PRINTCLIENT {
            wparam as Hdc
        } else {
            GetDC(window)
        };
        if !dc.is_null() {
            let saved = SaveDC(dc);
            let mut rect = Rect::default();
            SendMessageW(window, EM_GETRECT, 0, (&mut rect as *mut Rect) as isize);
            SelectObject(dc, SendMessageW(window, WM_GETFONT, 0, 0) as Hfont);
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, data.color);
            draw_text_in_rect(
                dc,
                &data.text,
                &mut rect,
                DT_NOPREFIX
                    | if data.multiline {
                        DT_WORDBREAK
                    } else {
                        DT_SINGLELINE | DT_VCENTER
                    },
            );
            if saved != 0 {
                RestoreDC(dc, saved);
            }
            if message != WM_PRINTCLIENT {
                ReleaseDC(window, dc);
            }
        }
    }
    result
}
