#![allow(unsafe_op_in_unsafe_fn)]

mod app_state;
mod async_scripts;
mod benchmark;
mod benchmark_capture;
mod browser_navigation;
mod chrome_controls;
mod chrome_paint;
mod document_activation;
mod document_navigation;
mod page_controls;
mod paint_index;
mod paint_primitives;
mod painting;
mod platform;
mod process_metrics;
mod reader_layout;
mod renderer_lifecycle;
mod rendering_resources;
mod resources;
mod runtime;
mod runtime_metrics;
mod task_manager;
mod viewport;
mod win32_helpers;
mod window_dispatch;
use app_state::BrowserState;
use benchmark::{BenchmarkRun, LaunchOptions};
use better_web_browser::branding::{BENCHMARK_ID, HOME_HTML, HOME_URL, PRODUCT_NAME};
use better_web_browser::document::{BlockKind, Document, Span, parse_html};
use better_web_browser::engine::{
    ControlKind, DecodedImage, DisplayItem, FontSpec, LayoutOutput, Page, PageResource,
    ScriptOutcome, ScriptRuntime, TextMeasurer, WebFont, layout_page_with_style_viewport,
};
use better_web_browser::metrics::BrowserMetrics;
use better_web_browser::navigation::{encode_www_form_component, normalize_user_input};
use better_web_browser::winhttp;
use chrome_controls::{ChromeLayout, Controls};
use document_activation::{LoadMessage, LoadedPage};
use platform::*;
use process_metrics::{process_cpu_ticks, process_memory};
use reader_layout::layout_document;
use rendering_resources::{
    DynamicFonts, FontKind, Fonts, GdiTextMeasurer, ImageBitmaps, WebFontResources,
};
use resources::DeferredResourcesMessage;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};
use viewport::{DrawItem, Surface};
use win32_helpers::*;
use window_dispatch::{chrome_control_proc, main_window_proc};

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

        let options = LaunchOptions::parse(process_started)?;
        let benchmark_is_hidden = options.benchmark.is_some();
        let (window_width_dip, window_height_dip) = options.window_dimensions();
        let metrics = Arc::new(BrowserMetrics::default());
        let state = Box::new(BrowserState::new(instance, metrics, options)?);
        let state_pointer = Box::into_raw(state);
        let class = wide(MAIN_CLASS);
        let title = wide(PRODUCT_NAME);
        let window_style = WS_OVERLAPPEDWINDOW
            | WS_VSCROLL
            | WS_CLIPCHILDREN
            | if benchmark_is_hidden { 0 } else { WS_VISIBLE };
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            window_style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale_dip(window_width_dip, initial_dpi),
            scale_dip(window_height_dip, initial_dpi),
            null_mut(),
            null_mut(),
            instance,
            state_pointer.cast(),
        );
        if window.is_null() {
            return Err(last_error("create browser window"));
        }

        ShowWindow(
            window,
            if benchmark_is_hidden {
                SW_HIDE
            } else {
                SW_SHOW
            },
        );
        UpdateWindow(window);
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
            if message.message == WM_KEYDOWN && message.wparam == VK_RETURN {
                let control_id = GetDlgCtrlID(message.hwnd);
                let parent = GetParent(message.hwnd);
                if control_id == ID_ADDRESS as i32 && !parent.is_null() {
                    SendMessageW(parent, WM_COMMAND, ID_GO, 0);
                    continue;
                } else if control_id >= ID_PAGE_CONTROL_BASE as i32 && !parent.is_null() {
                    let state =
                        (GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut BrowserState).as_ref();
                    let index = control_id as usize - ID_PAGE_CONTROL_BASE;
                    let is_textarea = state
                        .and_then(|state| state.page_controls.get(index))
                        .is_some_and(|control| control.spec.kind == ControlKind::TextArea);
                    if !is_textarea {
                        SendMessageW(
                            parent,
                            WM_COMMAND,
                            control_id as usize,
                            message.hwnd as isize,
                        );
                        continue;
                    }
                }
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
