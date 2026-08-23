mod controls;
mod metrics;
mod paint;
mod process_tree;

use self::metrics::{TaskMetricsView, browser_process_row, renderer_is_live, renderer_process_row};
use self::process_tree::PROCESS_ROW_HEIGHT_DIP;
use super::platform::*;
use super::process_metrics::{process_cpu_ticks, process_memory};
use super::renderer_lifecycle::SharedRendererRegistry;
use super::{
    create_font, format_bytes, format_duration, last_error, scale_dip, scaled_font_height, wide,
    window_dpi,
};
use better_web_browser::branding::PRODUCT_NAME;
use better_web_browser::metrics::BrowserMetrics;
use std::collections::{HashMap, HashSet};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};

const BASE_WINDOW_HEIGHT_DIP: i32 = 560;
const PROCESS_ROWS_AT_BASE_HEIGHT: usize = 3;

fn window_height_dip(process_rows: usize) -> i32 {
    let extra_rows = process_rows.saturating_sub(PROCESS_ROWS_AT_BASE_HEIGHT);
    BASE_WINDOW_HEIGHT_DIP.saturating_add(
        i32::try_from(extra_rows)
            .unwrap_or(i32::MAX)
            .saturating_mul(PROCESS_ROW_HEIGHT_DIP),
    )
}

pub(super) unsafe fn open(
    existing: Hwnd,
    parent: Hwnd,
    instance: Hinstance,
    dpi: u32,
    metrics: Arc<BrowserMetrics>,
    renderer_registry: SharedRendererRegistry,
) -> Result<Hwnd, String> {
    if !existing.is_null() && IsWindow(existing) != 0 {
        SetForegroundWindow(existing);
        return Ok(existing);
    }

    let process_rows = renderer_registry
        .lock()
        .map(|registry| registry.renderers.len().saturating_add(1))
        .unwrap_or_else(|poisoned| poisoned.into_inner().renderers.len().saturating_add(1));
    let initial_height = window_height_dip(process_rows);
    let state = Box::new(TaskManagerState::new(parent, metrics, renderer_registry));
    let pointer = Box::into_raw(state);
    let class = wide(TASK_CLASS);
    let title = wide(&format!("{PRODUCT_NAME} Task Manager"));
    let window = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class.as_ptr(),
        title.as_ptr(),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        scale_dip(600, dpi),
        scale_dip(initial_height, dpi),
        parent,
        null_mut(),
        instance,
        pointer.cast(),
    );
    if window.is_null() {
        drop(Box::from_raw(pointer));
        return Err(last_error("open task manager"));
    }

    ShowWindow(window, SW_SHOW);
    UpdateWindow(window);
    Ok(window)
}

struct TaskManagerFonts {
    title: Hfont,
    heading: Hfont,
    metric: Hfont,
    value: Hfont,
    body: Hfont,
    small: Hfont,
}

impl TaskManagerFonts {
    unsafe fn create(dpi: u32) -> Result<Self, String> {
        let fonts = Self {
            title: create_font(scaled_font_height(-24, dpi), 600, false, "Segoe UI"),
            heading: create_font(scaled_font_height(-13, dpi), 600, false, "Segoe UI"),
            metric: create_font(scaled_font_height(-30, dpi), 600, false, "Segoe UI"),
            value: create_font(scaled_font_height(-18, dpi), 600, false, "Segoe UI"),
            body: create_font(scaled_font_height(-16, dpi), 400, false, "Segoe UI"),
            small: create_font(scaled_font_height(-13, dpi), 400, false, "Segoe UI"),
        };
        if [
            fonts.title,
            fonts.heading,
            fonts.metric,
            fonts.value,
            fonts.body,
            fonts.small,
        ]
        .iter()
        .any(|font| font.is_null())
        {
            Err(last_error("create task manager fonts"))
        } else {
            Ok(fonts)
        }
    }
}

impl Drop for TaskManagerFonts {
    fn drop(&mut self) {
        unsafe {
            for font in [
                self.title,
                self.heading,
                self.metric,
                self.value,
                self.body,
                self.small,
            ] {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
        }
    }
}

struct TaskManagerState {
    parent: Hwnd,
    window: Hwnd,
    fonts: Option<TaskManagerFonts>,
    dpi: u32,
    metrics: Arc<BrowserMetrics>,
    renderer_registry: SharedRendererRegistry,
    started: Instant,
    previous_sample: Instant,
    previous_cpu_ticks: u64,
    previous_renderer_cpu_ticks: HashMap<u64, u64>,
    browser_cpu_percent: f64,
    cpu_percent: f64,
    logical_processors: usize,
    view: TaskMetricsView,
    selected_context: Option<u64>,
    end_process_button: Hwnd,
}

impl TaskManagerState {
    fn new(
        parent: Hwnd,
        metrics: Arc<BrowserMetrics>,
        renderer_registry: SharedRendererRegistry,
    ) -> Self {
        Self {
            parent,
            window: null_mut(),
            fonts: None,
            dpi: DEFAULT_DPI,
            metrics,
            renderer_registry,
            started: Instant::now(),
            previous_sample: Instant::now(),
            previous_cpu_ticks: process_cpu_ticks().unwrap_or(0),
            previous_renderer_cpu_ticks: HashMap::new(),
            browser_cpu_percent: 0.0,
            cpu_percent: 0.0,
            logical_processors: std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(1),
            view: TaskMetricsView::default(),
            selected_context: None,
            end_process_button: null_mut(),
        }
    }

    fn scale(&self, dip: i32) -> i32 {
        scale_dip(dip, self.dpi)
    }

    unsafe fn create(&mut self) -> Result<(), String> {
        self.dpi = window_dpi(self.window);
        self.fonts = Some(TaskManagerFonts::create(self.dpi)?);
        self.create_end_process_button()?;
        SetTimer(self.window, 1, 1_000, null());
        self.refresh();
        Ok(())
    }

    unsafe fn refresh(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.previous_sample).as_secs_f64();
        if let Some(current_ticks) = process_cpu_ticks() {
            if elapsed > 0.0 {
                let cpu_seconds =
                    current_ticks.saturating_sub(self.previous_cpu_ticks) as f64 / 10_000_000.0;
                self.browser_cpu_percent = (cpu_seconds / elapsed / self.logical_processors as f64
                    * 100.0)
                    .clamp(0.0, 100.0);
            }
            self.previous_cpu_ticks = current_ticks;
        }
        self.previous_sample = now;

        let memory = process_memory();
        let snapshot = self.metrics.snapshot();
        let registry = self
            .renderer_registry
            .lock()
            .map(|registry| registry.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        let mut handles = 0_u32;
        GetProcessHandleCount(GetCurrentProcess(), &mut handles);
        let mut processes = vec![browser_process_row(
            self.browser_cpu_percent,
            &memory,
            handles,
            self.started.elapsed(),
        )];
        let mut total_cpu_percent = self.browser_cpu_percent;
        let mut total_working_set = memory.working_set;
        let mut live_sessions = HashSet::new();
        for renderer in &registry.renderers {
            let renderer_cpu_percent = renderer
                .snapshot
                .as_ref()
                .filter(|_| renderer_is_live(renderer))
                .map(|snapshot| {
                    live_sessions.insert(snapshot.session_id);
                    let previous = self
                        .previous_renderer_cpu_ticks
                        .insert(snapshot.session_id, snapshot.cpu_ticks)
                        .unwrap_or(snapshot.cpu_ticks);
                    if elapsed > 0.0 {
                        let cpu_seconds =
                            snapshot.cpu_ticks.saturating_sub(previous) as f64 / 10_000_000.0;
                        cpu_seconds / elapsed / self.logical_processors as f64 * 100.0
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0)
                .clamp(0.0, 100.0);
            if renderer_is_live(renderer) {
                total_cpu_percent += renderer_cpu_percent;
                if let Some(snapshot) = renderer.snapshot.as_ref() {
                    total_working_set = total_working_set.saturating_add(snapshot.working_set);
                }
            }
            processes.push(renderer_process_row(renderer, renderer_cpu_percent));
        }
        self.previous_renderer_cpu_ticks
            .retain(|session, _| live_sessions.contains(session));
        self.cpu_percent = total_cpu_percent.clamp(0.0, 100.0);
        let process_count = 1 + registry
            .renderers
            .iter()
            .filter(|renderer| renderer_is_live(renderer))
            .count();
        self.view = TaskMetricsView {
            cpu: format!("{:.1}%", self.cpu_percent),
            working_set: format_bytes(total_working_set as u64),
            process_summary: format!("{process_count} LIVE"),
            processes,
            active_requests: snapshot.active_requests.to_string(),
            pages_completed: snapshot.pages_loaded.to_string(),
            failed_loads: snapshot.failed_loads.to_string(),
            downloaded: format_bytes(snapshot.bytes_downloaded),
            last_parse: format_duration(Duration::from_micros(snapshot.last_parse_micros)),
            draw_items: snapshot.retained_draw_items.to_string(),
        };
        self.update_end_process_state();
        InvalidateRect(self.window, null(), 0);
    }
}

pub(super) unsafe extern "system" fn window_proc(
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CreateStruct);
        let state = create.create_params as *mut TaskManagerState;
        (*state).window = window;
        SetWindowLongPtrW(window, GWLP_USERDATA, state as isize);
        return DefWindowProcW(window, message, wparam, lparam);
    }
    let state_pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut TaskManagerState;
    if state_pointer.is_null() {
        return DefWindowProcW(window, message, wparam, lparam);
    }
    let state = &mut *state_pointer;
    match message {
        WM_CREATE => {
            if state.create().is_err() {
                -1
            } else {
                0
            }
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam as *mut MinMaxInfo);
            info.min_track_size = Point {
                x: state.scale(600),
                y: state.scale(window_height_dip(state.view.processes.len())),
            };
            0
        }
        WM_SIZE => {
            state.position_end_process_button();
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
            if state.apply_dpi(dpi).is_err() {
                return -1;
            }
            state.position_end_process_button();
            InvalidateRect(window, null(), 0);
            0
        }
        WM_COMMAND if wparam & 0xffff == ID_TASK_END_PROCESS => {
            state.request_selected_renderer_termination();
            0
        }
        WM_LBUTTONUP => {
            let point = Point {
                x: (lparam as u16) as i16 as i32,
                y: ((lparam >> 16) as u16) as i16 as i32,
            };
            state.select_process_at(point);
            0
        }
        WM_TIMER => {
            state.refresh();
            0
        }
        WM_PAINT => {
            state.paint();
            0
        }
        WM_ERASEBKGND => 1,
        WM_CLOSE => {
            DestroyWindow(window);
            0
        }
        WM_DESTROY => {
            KillTimer(window, 1);
            if !state.parent.is_null() {
                PostMessageW(state.parent, WM_APP_TASK_CLOSED, 0, 0);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_manager_height_grows_with_large_process_trees() {
        assert_eq!(window_height_dip(2), 560);
        assert_eq!(window_height_dip(3), 560);
        assert_eq!(window_height_dip(4), 632);
        assert_eq!(window_height_dip(5), 704);
    }
}
