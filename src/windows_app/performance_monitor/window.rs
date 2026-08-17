//! Native child surface and clipboard export for the performance monitor.

use super::paint::{close_button_rect, copy_button_rect, format_ms, panel_size};
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

const REFRESH_INTERVAL_MS: u32 = 250;
const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x0002;

impl BrowserState {
    pub(in crate::windows_app) unsafe fn create_performance_window(
        &mut self,
    ) -> Result<(), String> {
        if self.benchmark.is_some() {
            return Ok(());
        }
        let class = wide(CLASS_NAME);
        let title = wide("");
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_CHILD,
            0,
            0,
            0,
            0,
            self.window,
            null_mut(),
            self.instance,
            null_mut(),
        );
        if window.is_null() {
            return Err(last_error("create performance monitor"));
        }
        self.performance_window = window;
        self.position_performance_window();
        if SetTimer(
            self.window,
            ID_PERFORMANCE_MONITOR_TIMER,
            REFRESH_INTERVAL_MS,
            null(),
        ) == 0
        {
            return Err(last_error("schedule performance monitor refresh"));
        }
        Ok(())
    }

    pub(in crate::windows_app) unsafe fn position_performance_window(&self) {
        if self.performance_window.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        if GetClientRect(self.window, &mut client) == 0 {
            return;
        }
        let size = panel_size(self);
        let margin = self.scale(12);
        let top = self.toolbar_height() + margin;
        let available_height = (self.chrome.status.top - top - margin).max(self.scale(180));
        let height = size.cy.min(available_height);
        SetWindowPos(
            self.performance_window,
            null_mut(),
            (client.right - size.cx - margin).max(margin),
            top,
            size.cx.min((client.right - margin * 2).max(1)),
            height,
            SWP_NOACTIVATE,
        );
    }

    unsafe fn copy_performance_diagnostics(&self) -> Result<(), String> {
        let snapshot = self.tabs.active().performance.snapshot(Instant::now());
        let captured_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let active_fps = snapshot
            .fps
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "idle".into());
        let last_scroll_fps = snapshot
            .last_scroll_fps
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "unavailable".into());
        let frame_intervals = snapshot
            .frame_history
            .iter()
            .map(|duration| format!("{:.1}", duration.as_secs_f64() * 1_000.0))
            .collect::<Vec<_>>()
            .join(", ");
        let report = format!(
            concat!(
                "Breeze performance diagnostics\r\n",
                "URL: {}\r\n",
                "Captured (Unix ms): {}\r\n",
                "Window: rolling 2 s\r\n",
                "FPS (active scroll): {}\r\n",
                "Last completed scroll FPS: {}\r\n",
                "Frame interval p95: {}\r\n",
                "Slowest interval: {}\r\n",
                "Long frames (>33 ms): {}\r\n",
                "Paint: {}\r\n",
                "JavaScript: {}\r\n",
                "Style: {}\r\n",
                "Layout: {}\r\n",
                "Resources: {}\r\n",
                "Frame intervals (ms, oldest to newest): {}\r\n"
            ),
            self.current_url().unwrap_or("about:blank"),
            captured_unix_ms,
            active_fps,
            last_scroll_fps,
            format_ms(snapshot.frame_p95),
            format_ms(snapshot.frame_maximum),
            snapshot.long_frames,
            format_ms(snapshot.paint_time),
            format_ms(snapshot.script_time),
            format_ms(snapshot.style_time),
            format_ms(snapshot.layout_time),
            format_ms(snapshot.resource_time),
            frame_intervals,
        );
        copy_unicode_text(self.window, &report)
    }
}

pub(in crate::windows_app) unsafe extern "system" fn window_proc(
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let parent = GetParent(window);
    let state_pointer = if parent.is_null() {
        null_mut()
    } else {
        GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut BrowserState
    };
    if state_pointer.is_null() {
        return DefWindowProcW(window, message, wparam, lparam);
    }
    let state = &mut *state_pointer;
    match message {
        WM_PAINT => {
            state.paint_performance_window(window);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => 0,
        WM_LBUTTONUP => {
            let point = Point {
                x: (lparam as u16) as i16 as i32,
                y: ((lparam >> 16) as u16) as i16 as i32,
            };
            let mut client: Rect = std::mem::zeroed();
            GetClientRect(window, &mut client);
            if contains(copy_button_rect(state, &client), point) {
                match state.copy_performance_diagnostics() {
                    Ok(()) => state.set_status("Performance diagnostics copied"),
                    Err(error) => state.set_status(&error),
                }
                InvalidateRect(window, null(), 0);
            } else if contains(close_button_rect(state, &client), point) {
                state.performance_panel_visible = false;
                ShowWindow(window, SW_HIDE);
                let counter = state.performance_counter_rect();
                InvalidateRect(parent, &counter, 0);
            }
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

fn contains(rectangle: Rect, point: Point) -> bool {
    point.x >= rectangle.left
        && point.x < rectangle.right
        && point.y >= rectangle.top
        && point.y < rectangle.bottom
}

unsafe fn copy_unicode_text(owner: Hwnd, text: &str) -> Result<(), String> {
    if OpenClipboard(owner) == 0 {
        return Err(last_error("open clipboard"));
    }
    if EmptyClipboard() == 0 {
        let error = last_error("clear clipboard");
        CloseClipboard();
        return Err(error);
    }
    let encoded = wide(text);
    let bytes = encoded.len().saturating_mul(size_of::<u16>());
    let memory = GlobalAlloc(GMEM_MOVEABLE, bytes);
    if memory.is_null() {
        let error = last_error("allocate clipboard text");
        CloseClipboard();
        return Err(error);
    }
    let destination = GlobalLock(memory) as *mut u16;
    if destination.is_null() {
        let error = last_error("lock clipboard text");
        GlobalFree(memory);
        CloseClipboard();
        return Err(error);
    }
    std::ptr::copy_nonoverlapping(encoded.as_ptr(), destination, encoded.len());
    GlobalUnlock(memory);
    if SetClipboardData(CF_UNICODETEXT, memory).is_null() {
        let error = last_error("publish clipboard text");
        GlobalFree(memory);
        CloseClipboard();
        return Err(error);
    }
    CloseClipboard();
    Ok(())
}
