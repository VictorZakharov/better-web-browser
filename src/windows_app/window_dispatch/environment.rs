//! Window focus, display topology, DPI and native viewport lifecycle.
use super::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{WM_DISPLAYCHANGE, WM_MOVE};

pub(super) unsafe fn dispatch(
    state: &mut BrowserState,
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Option<Lresult> {
    Some(match message {
        WM_MOVE | WM_DISPLAYCHANGE => {
            // WM_MOVE / WM_DISPLAYCHANGE: geometry can stay identical while the
            // active monitor's fractional refresh frequency changes.
            state.refresh_display_timing(message == WM_DISPLAYCHANGE);
            DefWindowProcW(window, message, wparam, lparam)
        }
        WM_ACTIVATE => {
            let active = wparam & 0xffff != 0;
            state.update_accessibility_window_focus(active);
            if active {
                state.sync_sensor_visibility();
            } else {
                // WA_INACTIVE is authoritative even if foreground ownership has
                // not changed by the time this message is dispatched.
                state.clear_visible_sensor_tab(state.tabs.active_id());
                state.exit_pointer_lock();
                // An owned permission dialog deactivates its browser window without
                // leaving the application. Actual external focus loss revokes capture.
                let activating = lparam as Hwnd;
                if activating.is_null() || GetAncestor(activating, GA_ROOTOWNER) != window {
                    state.retire_capture_for_window();
                }
            }
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
            state.refresh_display_timing(false);
            state.route_renderer_lifecycle(if wparam == 1 {
                better_web_browser::renderer_protocol::DocumentLifecycle::Hidden
            } else {
                better_web_browser::renderer_protocol::DocumentLifecycle::Active
            });
            state.handle_sensor_window_size(wparam);
            state.exit_pointer_lock();
            if wparam == 1 {
                // SIZE_MINIMIZED: capture must not persist in a hidden window.
                state.retire_capture_for_window();
                state.retire_wake_locks_for_window();
            }
            state.track_media_viewport_resize();
            state.mark_all_tab_layouts_dirty();
            state.resize_controls();
            state.rebuild_layout();
            state.refresh_accessibility_full();
            InvalidateRect(window, null(), 0);
            0
        }
        WM_SHOWWINDOW if wparam == 0 => {
            state.retire_capture_for_window();
            state.retire_wake_locks_for_window();
            DefWindowProcW(window, message, wparam, lparam)
        }
        WM_DPICHANGED => {
            state.exit_pointer_lock();
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
        color_scheme::WM_SETTINGCHANGE | color_scheme::WM_THEMECHANGED => {
            color_scheme::handle_change(state, window)
        }
        _ => return None,
    })
}
