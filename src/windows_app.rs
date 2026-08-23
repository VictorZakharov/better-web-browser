#![allow(unsafe_op_in_unsafe_fn)]
mod app_state;
mod benchmark;
mod benchmark_capture;
mod browser_app;
mod browser_navigation;
mod browser_window;
mod chrome_controls;
mod chrome_paint;
mod document_activation;
mod document_navigation;
mod document_state;
mod page_controls;
mod page_crash;
mod paint_index;
mod paint_primitives;
mod painting;
mod performance_monitor;
mod platform;
mod process_metrics;
mod profile;
mod reader_layout;
mod renderer_fetch;
mod renderer_lifecycle;
mod rendering_resources;
mod runtime;
mod scrolling;
mod tab_drag;
mod tab_management;
mod tab_paint;
mod tab_search;
mod tab_state;
mod tabs;
mod task_manager;
mod viewport;
mod win32_helpers;
mod window_dispatch;
use app_state::BrowserState;
use benchmark::{BenchmarkRun, LaunchOptions};
use better_web_browser::branding::{BENCHMARK_ID, HOME_HTML, HOME_URL, PRODUCT_NAME};
use better_web_browser::document::{BlockKind, Document, Span};
use better_web_browser::engine::{
    ControlKind, DecodedImage, DisplayItem, DisplayListDamage, FontSpec, LayoutOutput,
};
use better_web_browser::metrics::BrowserMetrics;
use better_web_browser::navigation::{encode_www_form_component, normalize_user_input};
use better_web_browser::winhttp;
use browser_app::BrowserApplication;
use browser_window::{BrowserWindowPlacement, create_browser_window};
use chrome_controls::{ChromeLayout, Controls};
use document_activation::{LoadMessage, LoadedPage, RendererLoadMetrics};
use performance_monitor::{PerformanceActivity, TabPerformance};
use platform::*;
use process_metrics::{process_cpu_ticks, process_memory};
use reader_layout::layout_document;
use rendering_resources::{DynamicFonts, FontKind, Fonts, GlyphBitmaps, ImageBitmaps};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use viewport::{DrawItem, Surface};
use win32_helpers::*;
use window_dispatch::{chrome_control_proc, dispatch_browser_input, main_window_proc};
pub fn run() -> Result<(), String> {
    unsafe {
        let process_started = Instant::now();
        // Per-monitor V2 keeps the custom chrome crisp as windows move between displays.
        // A failure is harmless when a host process has already selected a DPI mode.
        SetProcessDpiAwarenessContext(-4_isize as Handle);
        let initial_dpi = GetDpiForSystem().max(DEFAULT_DPI);
        let instance = GetModuleHandleW(null());
        if instance.is_null() {
            return Err(last_error("locate application module"));
        }
        register_class(instance, MAIN_CLASS, main_window_proc, COLOR_WINDOW)?;
        register_class(
            instance,
            TASK_CLASS,
            task_manager::window_proc,
            COLOR_WINDOW,
        )?;
        register_class(
            instance,
            TAB_SEARCH_CLASS,
            tab_search::window_proc,
            COLOR_WINDOW,
        )?;
        register_class(
            instance,
            performance_monitor::CLASS_NAME,
            performance_monitor::window_proc,
            COLOR_WINDOW,
        )?;

        let options = LaunchOptions::parse(process_started)?;
        let benchmark_is_hidden = options.benchmark.is_some();
        let (window_width_dip, window_height_dip) = options.window_dimensions();
        let metrics = Arc::new(BrowserMetrics::default());
        let app = BrowserApplication::new(instance, metrics)?;
        let state = BrowserState::new(Rc::clone(&app), options)?;
        let window = create_browser_window(
            state,
            BrowserWindowPlacement::initial(
                window_width_dip,
                window_height_dip,
                initial_dpi,
                !benchmark_is_hidden,
            ),
        )?;
        let state_pointer = app
            .state_pointer(window)
            .ok_or_else(|| "browser window did not retain its state".to_string())?;
        (*state_pointer).complete_startup();
        let mut message: Msg = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result == 0 {
                break;
            }
            if result < 0 {
                return Err(last_error("read window message"));
            }
            if let Some((browser_window, state_pointer)) = app.browser_for_message(message.hwnd)
                && dispatch_browser_input(&message, browser_window, &mut *state_pointer)
            {
                continue;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        Ok(())
    }
}

pub fn show_fatal_error(error: &str) {
    let message = wide(error);
    let title = wide(&format!("{PRODUCT_NAME} failed to start"));
    unsafe {
        MessageBoxW(null_mut(), message.as_ptr(), title.as_ptr(), 0x10);
    }
}

unsafe fn register_class(
    instance: Hinstance,
    name: &str,
    window_proc: WindowProc,
    background_color: usize,
) -> Result<(), String> {
    let name = wide(name);
    let class = WindowClassEx {
        size: size_of::<WindowClassEx>() as u32,
        style: 0x0002 | 0x0001,
        window_proc: Some(window_proc),
        class_extra: 0,
        window_extra: 0,
        instance,
        icon: null_mut(),
        cursor: LoadCursorW(null_mut(), int_resource(IDC_ARROW)),
        background: (background_color + 1) as Hbrush,
        menu_name: null(),
        class_name: name.as_ptr(),
        small_icon: null_mut(),
    };
    if RegisterClassExW(&class) == 0 {
        Err(last_error(&format!("register {name:?} window class")))
    } else {
        Ok(())
    }
}
