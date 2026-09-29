use super::super::tabs::{self, KeyModifiers, TabId};
use super::super::*;
use better_web_browser::renderer_protocol::TextEditIntent;
use std::cell::Cell;

thread_local! {
    static PAGE_EDIT_INTENT: Cell<TextEditIntent> = const { Cell::new(TextEditIntent::Unspecified) };
    static PAGE_EDIT_SELECTION: Cell<Option<PageEditSelection>> = const { Cell::new(None) };
    static PAGE_IME_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Copy)]
struct PageEditSelection {
    window: usize,
    start: u32,
    end: u32,
}

pub(in crate::windows_app) fn current_page_edit_intent() -> TextEditIntent {
    PAGE_EDIT_INTENT.with(Cell::get)
}

pub(in crate::windows_app) fn current_page_edit_selection(window: Hwnd) -> Option<(u32, u32)> {
    PAGE_EDIT_SELECTION.with(Cell::get).and_then(|selection| {
        (selection.window == window as usize).then_some((selection.start, selection.end))
    })
}

fn page_edit_intent(
    message: u32,
    wparam: usize,
    ime_active: bool,
    modifiers: KeyModifiers,
) -> TextEditIntent {
    if ime_active || modifiers.control || modifiers.alt {
        return TextEditIntent::Unspecified;
    }
    match message {
        WM_CHAR if wparam == 8 => TextEditIntent::DeleteContentBackward,
        WM_CHAR if wparam >= 32 && wparam != 127 => TextEditIntent::InsertText,
        WM_KEYDOWN if wparam == VK_DELETE && !modifiers.shift => {
            TextEditIntent::DeleteContentForward
        }
        _ => TextEditIntent::Unspecified,
    }
}

pub(super) unsafe fn reroute_tab_message(
    state: &BrowserState,
    id: TabId,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> bool {
    if state.tabs.contains(id) {
        return false;
    }
    // A completion can already be queued on the old HWND while a tab moves.
    // Forward its original payload unchanged to the tab's current owner.
    state
        .app
        .tab_router
        .destination(id)
        .filter(|window| *window != state.window as usize)
        .is_some_and(|window| PostMessageW(window as Hwnd, message, wparam, lparam) != 0)
}

pub(in crate::windows_app) unsafe fn dispatch_browser_input(
    message: &Msg,
    browser_window: Hwnd,
    state: &mut BrowserState,
) -> bool {
    if message.hwnd == state.task_window {
        return false;
    }
    let parent = GetParent(message.hwnd);
    let in_tab_search = !state.tab_search_window.is_null()
        && (message.hwnd == state.tab_search_window || parent == state.tab_search_window);
    if message.hwnd != browser_window && parent != browser_window && !in_tab_search {
        return false;
    }
    if matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN) {
        if message.wparam == VK_F11 {
            state.toggle_browser_fullscreen();
            return true;
        }
        if message.wparam == VK_ESCAPE && state.exit_pointer_lock() {
            return true;
        }
        if message.wparam == VK_ESCAPE && state.exit_page_fullscreen() {
            return true;
        }
        if is_diagnostics_shortcut(message.message, message.wparam) {
            state.toggle_performance_panel();
            return true;
        }
        let modifiers = KeyModifiers {
            control: GetKeyState(VK_CONTROL) < 0,
            shift: GetKeyState(VK_SHIFT) < 0,
            alt: GetKeyState(VK_MENU) < 0,
        };
        if let Some(shortcut) = tabs::shortcut_for_key(message.wparam, modifiers) {
            state.handle_shortcut(shortcut);
            return true;
        }
        if message.message == WM_KEYDOWN
            && GetDlgCtrlID(message.hwnd) == ID_ADDRESS as i32
            && is_select_all_shortcut(message.wparam, modifiers)
        {
            // EM_SETSEL documents (0, -1) as selecting all text in a Win32 edit control.
            SendMessageW(message.hwnd, EM_SETSEL, 0, -1);
            return true;
        }
        if in_tab_search && state.handle_tab_search_key(message.wparam) {
            return true;
        }
    }
    if matches!(
        message.message,
        WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP
    ) && !in_tab_search
    {
        state.route_renderer_keyboard(
            message.hwnd,
            message.message,
            message.wparam,
            message.lparam,
        );
    }
    if message.message != WM_KEYDOWN || message.wparam != VK_RETURN || parent.is_null() {
        return false;
    }
    let control_id = GetDlgCtrlID(message.hwnd);
    if control_id == ID_ADDRESS as i32 {
        SendMessageW(parent, WM_COMMAND, ID_GO, 0);
        return true;
    }
    false
}

pub(in crate::windows_app) unsafe extern "system" fn chrome_control_proc(
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
    _subclass_id: usize,
    control_id: usize,
) -> Lresult {
    match message {
        WM_MOUSEMOVE
            if control_id != ID_ADDRESS && GetWindowLongPtrW(window, GWLP_USERDATA) == 0 =>
        {
            SetWindowLongPtrW(window, GWLP_USERDATA, 1);
            InvalidateRect(window, null(), 0);
            let mut tracking = TrackMouseEventData {
                size: size_of::<TrackMouseEventData>() as u32,
                flags: TME_LEAVE,
                track_window: window,
                hover_time: 0,
            };
            TrackMouseEvent(&mut tracking);
        }
        WM_MOUSELEAVE if control_id != ID_ADDRESS => {
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            InvalidateRect(window, null(), 0);
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            let parent = GetParent(window);
            if !parent.is_null() {
                PostMessageW(parent, WM_APP_CHROME_INVALIDATE, 0, 0);
            }
            InvalidateRect(window, null(), 0);
        }
        _ => {}
    }
    DefSubclassProc(window, message, wparam, lparam)
}

pub(in crate::windows_app) unsafe extern "system" fn page_control_proc(
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
    _subclass_id: usize,
    control_id: usize,
) -> Lresult {
    if message == WM_IME_STARTCOMPOSITION {
        PAGE_IME_ACTIVE.with(|active| active.set(true));
    }
    if matches!(message, WM_SETFOCUS | WM_KILLFOCUS) {
        let parent = GetParent(window);
        let next = wparam as Hwnd;
        let moves_between_page_controls = message == WM_KILLFOCUS
            && !next.is_null()
            && GetParent(next) == parent
            && GetDlgCtrlID(next) >= ID_PAGE_CONTROL_BASE as i32;
        if !parent.is_null() && !moves_between_page_controls {
            SendMessageW(
                parent,
                WM_APP_PAGE_CONTROL_FOCUS,
                control_id,
                isize::from(message == WM_SETFOCUS),
            );
        }
    }
    let intent = page_edit_intent(
        message,
        wparam,
        PAGE_IME_ACTIVE.with(Cell::get),
        KeyModifiers {
            control: GetKeyState(VK_CONTROL) < 0,
            shift: GetKeyState(VK_SHIFT) < 0,
            alt: GetKeyState(VK_MENU) < 0,
        },
    );
    let previous = PAGE_EDIT_INTENT.with(|current| current.replace(intent));
    let selection = (intent != TextEditIntent::Unspecified)
        .then(|| {
            let (start, end) = edit_selection(window);
            PageEditSelection {
                window: window as usize,
                start,
                end,
            }
        })
        .filter(|selection| {
            selection.start <= selection.end
                && selection.end as usize
                    <= better_web_browser::limits::MAX_RENDERER_TEXT_INPUT_BYTES
        });
    let previous_selection = PAGE_EDIT_SELECTION.with(|current| current.replace(selection));
    let result = DefSubclassProc(window, message, wparam, lparam);
    PAGE_EDIT_SELECTION.with(|current| current.set(previous_selection));
    PAGE_EDIT_INTENT.with(|current| current.set(previous));
    if message == WM_IME_ENDCOMPOSITION {
        PAGE_IME_ACTIVE.with(|active| active.set(false));
    }
    result
}

fn is_select_all_shortcut(key: usize, modifiers: KeyModifiers) -> bool {
    key == b'A' as usize && modifiers.control && !modifiers.shift && !modifiers.alt
}

fn is_diagnostics_shortcut(message: u32, key: usize) -> bool {
    message == WM_KEYDOWN && key == VK_F12
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_edit_intent_only_claims_ordinary_non_ime_typing_and_deletion() {
        let plain = KeyModifiers::default();
        assert_eq!(
            page_edit_intent(WM_CHAR, 'a' as usize, false, plain),
            TextEditIntent::InsertText
        );
        assert_eq!(
            page_edit_intent(WM_CHAR, 8, false, plain),
            TextEditIntent::DeleteContentBackward
        );
        assert_eq!(
            page_edit_intent(WM_KEYDOWN, VK_DELETE, false, plain),
            TextEditIntent::DeleteContentForward
        );
        assert_eq!(
            page_edit_intent(WM_CHAR, 'a' as usize, true, plain),
            TextEditIntent::Unspecified
        );
        assert_eq!(
            page_edit_intent(0x0302, 0, false, plain),
            TextEditIntent::Unspecified
        );
        assert_eq!(
            page_edit_intent(WM_CHAR, 127, false, plain),
            TextEditIntent::Unspecified
        );
        assert_eq!(
            page_edit_intent(
                WM_KEYDOWN,
                VK_DELETE,
                false,
                KeyModifiers {
                    control: true,
                    ..plain
                }
            ),
            TextEditIntent::Unspecified
        );
    }

    #[test]
    fn address_select_all_does_not_shadow_tab_search() {
        let control = KeyModifiers {
            control: true,
            ..KeyModifiers::default()
        };
        assert!(is_select_all_shortcut(b'A' as usize, control));
        assert!(!is_select_all_shortcut(
            b'A' as usize,
            KeyModifiers {
                shift: true,
                ..control
            }
        ));
        assert!(!is_select_all_shortcut(b'B' as usize, control));
    }

    #[test]
    fn f12_toggles_diagnostics_only_on_key_down() {
        assert!(is_diagnostics_shortcut(WM_KEYDOWN, VK_F12));
        assert!(!is_diagnostics_shortcut(WM_KEYUP, VK_F12));
        assert!(!is_diagnostics_shortcut(WM_KEYDOWN, VK_RETURN));
    }

    #[test]
    fn fullscreen_keys_are_distinct_from_diagnostics() {
        assert_ne!(VK_F11, VK_F12);
        assert!(!is_diagnostics_shortcut(WM_KEYDOWN, VK_F11));
        assert!(!is_diagnostics_shortcut(WM_KEYDOWN, VK_ESCAPE));
    }
}
