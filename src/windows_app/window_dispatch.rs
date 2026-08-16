//! Win32 subclass and browser-window message dispatch.

use super::tabs::{self, KeyModifiers, TabId, TabStripHit};
use super::*;

pub(super) unsafe fn dispatch_browser_input(
    message: &Msg,
    browser_window: Hwnd,
    state: &mut BrowserState,
) -> bool {
    if message.hwnd == state.task_window {
        return false;
    }
    let parent = GetParent(message.hwnd);
    if message.hwnd != browser_window && parent != browser_window {
        return false;
    }
    if matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN) {
        let modifiers = KeyModifiers {
            control: GetKeyState(VK_CONTROL) < 0,
            shift: GetKeyState(VK_SHIFT) < 0,
            alt: GetKeyState(VK_MENU) < 0,
        };
        if let Some(shortcut) = tabs::shortcut_for_key(message.wparam, modifiers) {
            state.handle_shortcut(shortcut);
            return true;
        }
    }
    if message.message != WM_KEYDOWN || message.wparam != VK_RETURN || parent.is_null() {
        return false;
    }
    let control_id = GetDlgCtrlID(message.hwnd);
    if control_id == ID_ADDRESS as i32 {
        SendMessageW(parent, WM_COMMAND, ID_GO, 0);
        return true;
    }
    if control_id < ID_PAGE_CONTROL_BASE as i32 {
        return false;
    }
    let Some(index) = (control_id as usize).checked_sub(ID_PAGE_CONTROL_BASE) else {
        return false;
    };
    let Some(control) = state.page_controls.get(index) else {
        return false;
    };
    if control.spec.kind == ControlKind::TextArea {
        return false;
    }
    SendMessageW(
        parent,
        WM_COMMAND,
        control_id as usize,
        message.hwnd as isize,
    );
    true
}

pub(super) unsafe extern "system" fn chrome_control_proc(
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

pub(super) unsafe extern "system" fn main_window_proc(
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CreateStruct);
        let state = create.create_params as *mut BrowserState;
        (*state).window = window;
        SetWindowLongPtrW(window, GWLP_USERDATA, state as isize);
        return DefWindowProcW(window, message, wparam, lparam);
    }

    let state_pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut BrowserState;
    if state_pointer.is_null() {
        return DefWindowProcW(window, message, wparam, lparam);
    }
    let state = &mut *state_pointer;

    match message {
        WM_CREATE => {
            if state.create_controls().is_err() {
                return -1;
            }
            0
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam as *mut MinMaxInfo);
            info.min_track_size = Point {
                x: state.scale(500),
                y: state.scale(360),
            };
            0
        }
        WM_SIZE => {
            state.track_media_viewport_resize();
            state.mark_all_tab_layouts_dirty();
            state.resize_controls();
            state.rebuild_layout();
            InvalidateRect(window, null(), 0);
            0
        }
        WM_DPICHANGED => {
            let dpi = (wparam & 0xffff) as u32;
            let suggested = &*(lparam as *const Rect);
            SetWindowPos(
                window,
                null_mut(),
                suggested.left,
                suggested.top,
                suggested.width(),
                suggested.height(),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if let Err(error) = state.apply_dpi(dpi) {
                state.set_status(&error);
            }
            state.resize_controls();
            state.rebuild_layout();
            InvalidateRect(window, null(), 0);
            0
        }
        WM_COMMAND => {
            let id = wparam & 0xffff;
            let notification = (wparam >> 16) & 0xffff;
            match id {
                ID_BACK => state.go_back(),
                ID_FORWARD => state.go_forward(),
                ID_RELOAD => state.reload(),
                ID_GO => state.navigate_from_address(),
                ID_TASK_MANAGER => state.open_task_manager(),
                ID_READER => state.toggle_reader(),
                ID_PAGE_CONTROL_BASE.. => state.activate_page_control(id, notification),
                _ => {}
            }
            0
        }
        WM_DRAWITEM => {
            let item = &*(lparam as *const DrawItemStruct);
            if matches!(
                item.control_id as usize,
                ID_BACK | ID_FORWARD | ID_RELOAD | ID_GO | ID_TASK_MANAGER | ID_READER
            ) {
                state.paint_chrome_button(item);
                1
            } else if let Some(index) = (item.control_id as usize).checked_sub(ID_PAGE_CONTROL_BASE)
                && index < state.page_controls.len()
            {
                state.paint_page_button(item, index);
                1
            } else {
                DefWindowProcW(window, message, wparam, lparam)
            }
        }
        WM_CTLCOLOREDIT if lparam as Hwnd == state.controls.address => {
            let dc = wparam as Hdc;
            SetTextColor(dc, CHROME_THEME.text);
            SetBkColor(dc, CHROME_THEME.field);
            state.omnibox_brush as Lresult
        }
        WM_CTLCOLOREDIT => {
            let control_window = lparam as Hwnd;
            if let Some(control) = state
                .page_controls
                .iter()
                .find(|control| control.window == control_window)
            {
                let dc = wparam as Hdc;
                SetTextColor(dc, control.spec.text_color.to_colorref());
                SetBkColor(dc, control.spec.background_color.to_colorref());
                control.brush as Lresult
            } else {
                DefWindowProcW(window, message, wparam, lparam)
            }
        }
        WM_TIMER if wparam == ID_SCRIPT_RUNTIME_TIMER => {
            state.pump_script_runtime();
            0
        }
        WM_TIMER if wparam == ID_RENDERER_MONITOR_TIMER => {
            state.poll_renderers();
            0
        }
        WM_APP_CHROME_INVALIDATE => {
            let toolbar = Rect {
                left: 0,
                top: 0,
                right: state.chrome.status.right,
                bottom: state.toolbar_height(),
            };
            InvalidateRect(window, &toolbar, 0);
            0
        }
        WM_APP_PAGE_LOADED => {
            let message = Box::from_raw(lparam as *mut LoadMessage);
            if let Some(id) = TabId::from_message(wparam) {
                state.route_navigation_message(id, *message);
            }
            0
        }
        WM_APP_DEFERRED_RESOURCES => {
            let message = Box::from_raw(lparam as *mut DeferredResourcesMessage);
            if let Some(id) = TabId::from_message(wparam) {
                state.route_resource_message(id, *message);
            }
            0
        }
        WM_APP_ASYNC_SCRIPT => {
            let message = Box::from_raw(lparam as *mut async_scripts::AsyncScriptMessage);
            if let Some(id) = TabId::from_message(wparam) {
                state.route_async_script_message(id, *message);
            }
            0
        }
        WM_APP_RENDERER_LAUNCHED => {
            if let Some(id) = TabId::from_message(wparam) {
                state.finish_renderer_launch(id);
            }
            0
        }
        WM_APP_TASK_CLOSED => {
            state.task_window = null_mut();
            0
        }
        WM_APP_BENCHMARK_FINISH => {
            state.finish_benchmark();
            0
        }
        WM_PAINT => {
            state.paint();
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as u16) as i16 as i32;
            state.scroll_to(state.scroll_y - (delta / 120) * 126);
            0
        }
        WM_VSCROLL => {
            state.handle_scroll((wparam & 0xffff) as u16);
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam as u16) as i16 as i32;
            let y = ((lparam >> 16) as u16) as i16 as i32;
            if !state.handle_tab_strip_click(x, y) {
                state.click_content(x, y, wparam & MK_CONTROL != 0);
            }
            0
        }
        WM_MBUTTONUP => {
            let x = (lparam as u16) as i16 as i32;
            let y = ((lparam >> 16) as u16) as i16 as i32;
            let mut client: Rect = std::mem::zeroed();
            GetClientRect(window, &mut client);
            match state.tab_strip_layout(client.right).hit_test(x, y) {
                Some(TabStripHit::Activate(id) | TabStripHit::Close(id)) => state.close_tab(id),
                Some(TabStripHit::NewTab) => state.new_tab(),
                None => state.click_content(x, y, true),
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(window);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            let result = DefWindowProcW(window, message, wparam, lparam);
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_pointer));
            result
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}
