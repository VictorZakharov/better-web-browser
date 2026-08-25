//! Win32 subclass and browser-window message dispatch.

mod input;

use super::tabs::{KeyModifiers, TabId, TabStripHit};
use super::*;
use input::reroute_tab_message;
pub(super) use input::{chrome_control_proc, dispatch_browser_input, page_control_proc};
use std::panic::{AssertUnwindSafe, catch_unwind};

pub(super) unsafe extern "system" fn main_window_proc(
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CreateStruct);
        let state = create.create_params as *mut BrowserState;
        let state_ref = &mut *state;
        state_ref.window = window;
        state_ref.app.register_window(window);
        for tab in state_ref.tabs.iter() {
            state_ref.app.tab_router.bind(tab.id, window);
        }
        SetWindowLongPtrW(window, GWLP_USERDATA, state as isize);
        return DefWindowProcW(window, message, wparam, lparam);
    }

    let state_pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut BrowserState;
    if state_pointer.is_null() {
        return DefWindowProcW(window, message, wparam, lparam);
    }
    let state = &mut *state_pointer;

    if matches!(message, WM_CREATE | WM_CLOSE | WM_DESTROY | WM_NCDESTROY) {
        return dispatch_window_message(state_pointer, state, window, message, wparam, lparam);
    }
    match catch_unwind(AssertUnwindSafe(|| {
        dispatch_window_message(state_pointer, state, window, message, wparam, lparam)
    })) {
        Ok(result) => result,
        Err(payload) => {
            let id = if matches!(
                message,
                WM_APP_PAGE_LOADED | WM_APP_RENDERER_LAUNCHED | WM_APP_RENDERER_FETCH_COMPLETE
            ) {
                TabId::from_message(wparam).unwrap_or_else(|| state.tabs.active_id())
            } else {
                state.tabs.active_id()
            };
            state.contain_page_engine_failure(id, super::page_crash::panic_detail(payload));
            0
        }
    }
}

unsafe fn dispatch_window_message(
    state_pointer: *mut BrowserState,
    state: &mut BrowserState,
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    match message {
        WM_CREATE => {
            if state.create_controls().is_err() {
                return -1;
            }
            state.initialize_accessibility();
            0
        }
        WM_GETOBJECT => state
            .handle_accessibility_getobject(wparam, lparam)
            .unwrap_or_else(|| DefWindowProcW(window, message, wparam, lparam)),
        WM_ACTIVATE => {
            state.update_accessibility_window_focus(wparam & 0xffff != 0);
            DefWindowProcW(window, message, wparam, lparam)
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam as *mut MinMaxInfo);
            info.min_track_size = Point {
                x: state.scale(500),
                y: state.scale(360),
            };
            0
        }
        WM_SETCURSOR if state.apply_page_cursor_for_hit_test(lparam) => 1,
        WM_SIZE => {
            state.track_media_viewport_resize();
            state.mark_all_tab_layouts_dirty();
            state.resize_controls();
            state.rebuild_layout();
            state.refresh_accessibility_full();
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
            state.refresh_accessibility_full();
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
        WM_TIMER if wparam == ID_RENDERER_RUNTIME_TIMER => {
            state.pump_script_runtime();
            0
        }
        WM_TIMER if wparam == ID_RENDERER_MONITOR_TIMER => {
            state.poll_renderers();
            0
        }
        WM_TIMER if wparam == ID_PERFORMANCE_MONITOR_TIMER => {
            state.refresh_performance_monitor();
            0
        }
        WM_TIMER if wparam == ID_SCROLL_ANIMATION_TIMER => {
            state.tick_scroll_animation();
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
            state.refresh_accessibility_chrome();
            0
        }
        WM_APP_ACCESSIBILITY_ACTION => {
            state.dispatch_accessibility_actions();
            0
        }
        WM_APP_PAGE_CONTROL_FOCUS => {
            if !state.suppress_page_control_focus {
                state.route_page_control_focus(wparam, lparam != 0);
                state.refresh_accessibility_chrome();
            }
            0
        }
        WM_APP_PAGE_LOADED => {
            if let Some(id) = TabId::from_message(wparam) {
                if reroute_tab_message(state, id, message, wparam, lparam) {
                    return 0;
                }
                let message = Box::from_raw(lparam as *mut LoadMessage);
                if state.tabs.contains(id) {
                    state.route_navigation_message(id, *message);
                }
            } else {
                drop(Box::from_raw(lparam as *mut LoadMessage));
            }
            0
        }
        WM_APP_RENDERER_FETCH_COMPLETE => {
            if let Some(id) = TabId::from_message(wparam) {
                if reroute_tab_message(state, id, message, wparam, lparam) {
                    return 0;
                }
                let completion =
                    Box::from_raw(lparam as *mut renderer_fetch::RendererFetchCompletion);
                if state.tabs.contains(id) {
                    state.route_renderer_fetch_completion(id, *completion);
                }
            } else {
                drop(Box::from_raw(
                    lparam as *mut renderer_fetch::RendererFetchCompletion,
                ));
            }
            0
        }
        WM_APP_RENDERER_LAUNCHED => {
            if let Some(id) = TabId::from_message(wparam) {
                if reroute_tab_message(state, id, message, wparam, lparam) {
                    return 0;
                }
                if state.tabs.contains(id) {
                    state.finish_renderer_launch(id);
                }
            }
            0
        }
        WM_APP_TASK_CLOSED => {
            state.task_window = null_mut();
            0
        }
        WM_APP_TASK_TERMINATE_RENDERER => {
            if let Some(id) = TabId::from_message(wparam) {
                state.terminate_renderer_for(id);
            }
            0
        }
        WM_APP_TAB_SEARCH_CLOSED => {
            state.tab_search_window = null_mut();
            state.tab_search_edit = null_mut();
            0
        }
        WM_APP_BENCHMARK_FINISH => {
            state.finish_benchmark();
            0
        }
        WM_APP_EARLY_SCROLL_TICK => {
            state.handle_early_scroll_tick(wparam);
            0
        }
        WM_PAINT => {
            state.paint();
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as u16) as i16 as i32;
            state.queue_wheel_scroll(delta);
            0
        }
        WM_MOUSEMOVE => {
            let point = Point {
                x: (lparam as u16) as i16 as i32,
                y: ((lparam >> 16) as u16) as i16 as i32,
            };
            state.update_tab_hover(point);
            state.track_pointer_leave();
            let over_tab_strip = state.update_tab_pointer(point);
            if over_tab_strip {
                state.reset_pointer_cursor();
            }
            if over_tab_strip
                || state.update_reader_pointer_cursor(point)
                || state.route_content_pointer(
                    point.x,
                    point.y,
                    better_web_browser::renderer_protocol::PointerPhase::Move,
                    better_web_browser::renderer_protocol::PointerButton::None,
                    wparam,
                )
            {
                0
            } else {
                DefWindowProcW(window, message, wparam, lparam)
            }
        }
        WM_MOUSELEAVE => {
            state.reset_pointer_cursor();
            0
        }
        WM_LBUTTONDOWN => {
            let point = Point {
                x: (lparam as u16) as i16 as i32,
                y: ((lparam >> 16) as u16) as i16 as i32,
            };
            let modifiers = KeyModifiers {
                control: wparam & MK_CONTROL != 0,
                shift: wparam & MK_SHIFT != 0,
                alt: GetKeyState(VK_MENU) < 0,
            };
            if state.begin_tab_pointer(point, modifiers) {
                0
            } else if state.route_content_pointer(
                point.x,
                point.y,
                better_web_browser::renderer_protocol::PointerPhase::Down,
                better_web_browser::renderer_protocol::PointerButton::Primary,
                wparam,
            ) {
                SetFocus(window);
                0
            } else {
                DefWindowProcW(window, message, wparam, lparam)
            }
        }
        WM_MBUTTONDOWN | WM_RBUTTONDOWN => {
            let x = (lparam as u16) as i16 as i32;
            let y = ((lparam >> 16) as u16) as i16 as i32;
            let button = if message == WM_MBUTTONDOWN {
                better_web_browser::renderer_protocol::PointerButton::Middle
            } else {
                better_web_browser::renderer_protocol::PointerButton::Secondary
            };
            if state.route_content_pointer(
                x,
                y,
                better_web_browser::renderer_protocol::PointerPhase::Down,
                button,
                wparam,
            ) {
                0
            } else {
                DefWindowProcW(window, message, wparam, lparam)
            }
        }
        WM_VSCROLL => {
            state.handle_scroll((wparam & 0xffff) as u16);
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam as u16) as i16 as i32;
            let y = ((lparam >> 16) as u16) as i16 as i32;
            if state.toggle_performance_at(x, y) {
                return 0;
            }
            if state.finish_tab_pointer(Point { x, y }) {
                return 0;
            }
            if !state.handle_tab_strip_click(x, y)
                && !state.route_content_pointer(
                    x,
                    y,
                    better_web_browser::renderer_protocol::PointerPhase::Up,
                    better_web_browser::renderer_protocol::PointerButton::Primary,
                    wparam,
                )
            {
                state.click_content(x, y, wparam & MK_CONTROL != 0);
            }
            0
        }
        WM_CANCELMODE | WM_CAPTURECHANGED => {
            state.cancel_tab_pointer();
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
                Some(TabStripHit::SearchTabs) => state.toggle_tab_search(),
                None => {
                    if !state.route_content_pointer(
                        x,
                        y,
                        better_web_browser::renderer_protocol::PointerPhase::Up,
                        better_web_browser::renderer_protocol::PointerButton::Middle,
                        wparam,
                    ) {
                        state.click_content(x, y, true);
                    }
                }
            }
            0
        }
        WM_RBUTTONUP => {
            let x = (lparam as u16) as i16 as i32;
            let y = ((lparam >> 16) as u16) as i16 as i32;
            if state.route_content_pointer(
                x,
                y,
                better_web_browser::renderer_protocol::PointerPhase::Up,
                better_web_browser::renderer_protocol::PointerButton::Secondary,
                wparam,
            ) {
                0
            } else {
                DefWindowProcW(window, message, wparam, lparam)
            }
        }
        WM_CLOSE => {
            DestroyWindow(window);
            0
        }
        WM_DESTROY => {
            KillTimer(window, ID_PERFORMANCE_MONITOR_TIMER);
            KillTimer(window, ID_SCROLL_ANIMATION_TIMER);
            if !state.tab_search_window.is_null() {
                DestroyWindow(state.tab_search_window);
            }
            if state.app.window_count() == 1 {
                PostQuitMessage(0);
            }
            0
        }
        WM_NCDESTROY => {
            state.app.unregister_window(window);
            let result = DefWindowProcW(window, message, wparam, lparam);
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_pointer));
            result
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}
