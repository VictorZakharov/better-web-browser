mod paint;

use super::platform::*;
use super::process_metrics::{process_cpu_ticks, process_memory};
use super::{
    create_font, format_bytes, format_duration, last_error, scale_dip, scaled_font_height, wide,
    window_dpi,
};
use better_web_browser::branding::PRODUCT_NAME;
use better_web_browser::metrics::BrowserMetrics;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) unsafe fn open(
    existing: Hwnd,
    parent: Hwnd,
    instance: Hinstance,
    dpi: u32,
    metrics: Arc<BrowserMetrics>,
) -> Result<Hwnd, String> {
    if !existing.is_null() && IsWindow(existing) != 0 {
        SetForegroundWindow(existing);
        return Ok(existing);
    }

    let state = Box::new(TaskManagerState::new(parent, metrics));
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
        scale_dip(560, dpi),
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

struct TaskMetricsView {
    cpu: String,
    working_set: String,
    private_memory: String,
    peak_working_set: String,
    handles: String,
    uptime: String,
    active_requests: String,
    pages_completed: String,
    failed_loads: String,
    downloaded: String,
    last_parse: String,
    draw_items: String,
}

impl Default for TaskMetricsView {
    fn default() -> Self {
        Self {
            cpu: "0.0%".into(),
            working_set: "—".into(),
            private_memory: "—".into(),
            peak_working_set: "—".into(),
            handles: "—".into(),
            uptime: "0 ms".into(),
            active_requests: "0".into(),
            pages_completed: "0".into(),
            failed_loads: "0".into(),
            downloaded: "0 B".into(),
            last_parse: "0 μs".into(),
            draw_items: "0".into(),
        }
    }
}

struct TaskManagerState {
    parent: Hwnd,
    window: Hwnd,
    fonts: Option<TaskManagerFonts>,
    dpi: u32,
    metrics: Arc<BrowserMetrics>,
    started: Instant,
    previous_sample: Instant,
    previous_cpu_ticks: u64,
    cpu_percent: f64,
    logical_processors: usize,
    view: TaskMetricsView,
}

impl TaskManagerState {
    fn new(parent: Hwnd, metrics: Arc<BrowserMetrics>) -> Self {
        Self {
            parent,
            window: null_mut(),
            fonts: None,
            dpi: DEFAULT_DPI,
            metrics,
            started: Instant::now(),
            previous_sample: Instant::now(),
            previous_cpu_ticks: process_cpu_ticks().unwrap_or(0),
            cpu_percent: 0.0,
            logical_processors: std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(1),
            view: TaskMetricsView::default(),
        }
    }

    fn scale(&self, dip: i32) -> i32 {
        scale_dip(dip, self.dpi)
    }

    unsafe fn create(&mut self) -> Result<(), String> {
        self.dpi = window_dpi(self.window);
        self.fonts = Some(TaskManagerFonts::create(self.dpi)?);
        SetTimer(self.window, 1, 1_000, null());
        self.refresh();
        Ok(())
    }

    unsafe fn apply_dpi(&mut self, dpi: u32) -> Result<(), String> {
        let dpi = dpi.max(DEFAULT_DPI);
        if dpi != self.dpi {
            self.fonts = Some(TaskManagerFonts::create(dpi)?);
            self.dpi = dpi;
        }
        Ok(())
    }

    unsafe fn refresh(&mut self) {
        let now = Instant::now();
        if let Some(current_ticks) = process_cpu_ticks() {
            let elapsed = now.duration_since(self.previous_sample).as_secs_f64();
            if elapsed > 0.0 {
                let cpu_seconds =
                    current_ticks.saturating_sub(self.previous_cpu_ticks) as f64 / 10_000_000.0;
                self.cpu_percent = (cpu_seconds / elapsed / self.logical_processors as f64 * 100.0)
                    .clamp(0.0, 100.0);
            }
            self.previous_cpu_ticks = current_ticks;
        }
        self.previous_sample = now;

        let memory = process_memory();
        let snapshot = self.metrics.snapshot();
        let mut handles = 0_u32;
        GetProcessHandleCount(GetCurrentProcess(), &mut handles);
        self.view = TaskMetricsView {
            cpu: format!("{:.1}%", self.cpu_percent),
            working_set: format_bytes(memory.working_set as u64),
            private_memory: format_bytes(memory.private_usage as u64),
            peak_working_set: format_bytes(memory.peak_working_set as u64),
            handles: handles.to_string(),
            uptime: format_duration(self.started.elapsed()),
            active_requests: snapshot.active_requests.to_string(),
            pages_completed: snapshot.pages_loaded.to_string(),
            failed_loads: snapshot.failed_loads.to_string(),
            downloaded: format_bytes(snapshot.bytes_downloaded),
            last_parse: format_duration(Duration::from_micros(snapshot.last_parse_micros)),
            draw_items: snapshot.retained_draw_items.to_string(),
        };
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
                x: state.scale(480),
                y: state.scale(500),
            };
            0
        }
        WM_SIZE => {
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
            InvalidateRect(window, null(), 0);
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
