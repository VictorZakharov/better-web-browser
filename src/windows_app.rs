#![allow(unsafe_op_in_unsafe_fn)]

use better_web_browser::branding::{BENCHMARK_ID, HOME_HTML, HOME_URL, PRODUCT_NAME};
use better_web_browser::document::{BlockKind, Document, Span, parse_html};
use better_web_browser::engine::{
    ControlKind, DecodedImage, DisplayItem, FontSpec, LayoutOutput, Page, PageResource, RectF,
    TextMeasurer, layout_page,
};
use better_web_browser::metrics::BrowserMetrics;
use better_web_browser::navigation::{encode_www_form_component, normalize_user_input};
use better_web_browser::winhttp;
use std::collections::HashMap;
use std::ffi::c_void;
use std::io;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};

type Handle = *mut c_void;
type Hwnd = Handle;
type Hinstance = Handle;
type Hicon = Handle;
type Hcursor = Handle;
type Hbrush = Handle;
type Hmenu = Handle;
type Hdc = Handle;
type Hgdiobj = Handle;
type Hfont = Handle;
type Hrgn = Handle;
type Hbitmap = Handle;
type Lresult = isize;
type Wparam = usize;
type Lparam = isize;

const MAIN_CLASS: &str = "BetterWebBrowserMainWindow";
const TASK_CLASS: &str = "BetterWebBrowserTaskManagerWindow";

const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const WM_VSCROLL: u32 = 0x0115;
const WM_KEYDOWN: u32 = 0x0100;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_NCCREATE: u32 = 0x0081;
const WM_NCDESTROY: u32 = 0x0082;
const WM_SETFONT: u32 = 0x0030;
const EM_SETCUEBANNER: u32 = 0x1501;
const WM_APP: u32 = 0x8000;
const WM_APP_PAGE_LOADED: u32 = WM_APP + 1;
const WM_APP_TASK_CLOSED: u32 = WM_APP + 2;
const WM_APP_BENCHMARK_FINISH: u32 = WM_APP + 3;

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_PASSWORD: u32 = 0x0020;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const SS_LEFT: u32 = 0x0000;
const BS_PUSHBUTTON: u32 = 0x0000;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;

const SW_SHOW: i32 = 5;
const CW_USEDEFAULT: i32 = i32::MIN;
const GWLP_USERDATA: i32 = -21;
const COLOR_WINDOW: usize = 5;
const IDC_ARROW: u16 = 32512;
const TRANSPARENT: i32 = 1;
const VK_RETURN: usize = 0x0D;

const SIF_RANGE: u32 = 0x0001;
const SIF_PAGE: u32 = 0x0002;
const SIF_POS: u32 = 0x0004;
const SIF_TRACKPOS: u32 = 0x0010;
const SB_VERT: i32 = 1;
const SB_LINEUP: u16 = 0;
const SB_LINEDOWN: u16 = 1;
const SB_PAGEUP: u16 = 2;
const SB_PAGEDOWN: u16 = 3;
const SB_THUMBPOSITION: u16 = 4;
const SB_THUMBTRACK: u16 = 5;
const SB_TOP: u16 = 6;
const SB_BOTTOM: u16 = 7;

const ID_BACK: usize = 1001;
const ID_FORWARD: usize = 1002;
const ID_RELOAD: usize = 1003;
const ID_ADDRESS: usize = 1004;
const ID_GO: usize = 1005;
const ID_TASK_MANAGER: usize = 1006;
const ID_READER: usize = 1007;
const ID_PAGE_CONTROL_BASE: usize = 2000;

const TOOLBAR_HEIGHT: i32 = 50;
const STATUS_HEIGHT: i32 = 25;
const CONTENT_MARGIN: i32 = 28;
const MAX_READING_WIDTH: i32 = 920;
const SW_HIDE: i32 = 0;
const DIB_RGB_COLORS: u32 = 0;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct Size {
    cx: i32,
    cy: i32,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
    time: u32,
    point: Point,
    private: u32,
}

type WindowProc = unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult;

#[repr(C)]
struct WindowClassEx {
    size: u32,
    style: u32,
    window_proc: Option<WindowProc>,
    class_extra: i32,
    window_extra: i32,
    instance: Hinstance,
    icon: Hicon,
    cursor: Hcursor,
    background: Hbrush,
    menu_name: *const u16,
    class_name: *const u16,
    small_icon: Hicon,
}

#[repr(C)]
struct CreateStruct {
    create_params: *mut c_void,
    instance: Hinstance,
    menu: Hmenu,
    parent: Hwnd,
    height: i32,
    width: i32,
    y: i32,
    x: i32,
    style: i32,
    name: *const u16,
    class: *const u16,
    extended_style: u32,
}

#[repr(C)]
struct PaintStruct {
    dc: Hdc,
    erase: i32,
    paint: Rect,
    restore: i32,
    inc_update: i32,
    reserved: [u8; 32],
}

#[repr(C)]
struct ScrollInfo {
    size: u32,
    mask: u32,
    min: i32,
    max: i32,
    page: u32,
    position: i32,
    track_position: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C)]
struct ProcessMemoryCountersEx {
    size: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_usage: usize,
}

#[repr(C)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_pixels_per_meter: i32,
    y_pixels_per_meter: i32,
    colors_used: u32,
    colors_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BlendFunction {
    operation: u8,
    flags: u8,
    source_constant_alpha: u8,
    alpha_format: u8,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassExW(class: *const WindowClassEx) -> u16;
    fn CreateWindowExW(
        extended_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: Hmenu,
        instance: Hinstance,
        parameter: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(window: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn ShowWindow(window: Hwnd, command: i32) -> i32;
    fn UpdateWindow(window: Hwnd) -> i32;
    fn GetMessageW(message: *mut Msg, window: Hwnd, min: u32, max: u32) -> i32;
    fn TranslateMessage(message: *const Msg) -> i32;
    fn DispatchMessageW(message: *const Msg) -> Lresult;
    fn PostQuitMessage(exit_code: i32);
    fn PostMessageW(window: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> i32;
    fn SendMessageW(window: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn SetWindowLongPtrW(window: Hwnd, index: i32, value: isize) -> isize;
    fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
    fn LoadCursorW(instance: Hinstance, cursor_name: *const u16) -> Hcursor;
    fn BeginPaint(window: Hwnd, paint: *mut PaintStruct) -> Hdc;
    fn EndPaint(window: Hwnd, paint: *const PaintStruct) -> i32;
    fn GetClientRect(window: Hwnd, rectangle: *mut Rect) -> i32;
    fn MoveWindow(window: Hwnd, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
    fn InvalidateRect(window: Hwnd, rectangle: *const Rect, erase: i32) -> i32;
    fn SetWindowTextW(window: Hwnd, text: *const u16) -> i32;
    fn GetWindowTextLengthW(window: Hwnd) -> i32;
    fn GetWindowTextW(window: Hwnd, text: *mut u16, maximum: i32) -> i32;
    fn SetFocus(window: Hwnd) -> Hwnd;
    fn GetParent(window: Hwnd) -> Hwnd;
    fn GetDlgCtrlID(window: Hwnd) -> i32;
    fn DestroyWindow(window: Hwnd) -> i32;
    fn IsWindow(window: Hwnd) -> i32;
    fn SetForegroundWindow(window: Hwnd) -> i32;
    fn EnableWindow(window: Hwnd, enabled: i32) -> i32;
    fn SetTimer(window: Hwnd, id: usize, interval: u32, callback: *const c_void) -> usize;
    fn KillTimer(window: Hwnd, id: usize) -> i32;
    fn GetDC(window: Hwnd) -> Hdc;
    fn ReleaseDC(window: Hwnd, dc: Hdc) -> i32;
    fn FillRect(dc: Hdc, rectangle: *const Rect, brush: Hbrush) -> i32;
    fn SetScrollInfo(window: Hwnd, bar: i32, info: *const ScrollInfo, redraw: i32) -> i32;
    fn GetScrollInfo(window: Hwnd, bar: i32, info: *mut ScrollInfo) -> i32;
    fn MessageBoxW(window: Hwnd, text: *const u16, caption: *const u16, kind: u32) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike_out: u32,
        character_set: u32,
        output_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch_and_family: u32,
        face: *const u16,
    ) -> Hfont;
    fn SelectObject(dc: Hdc, object: Hgdiobj) -> Hgdiobj;
    fn DeleteObject(object: Hgdiobj) -> i32;
    fn SetTextColor(dc: Hdc, color: u32) -> u32;
    fn SetBkMode(dc: Hdc, mode: i32) -> i32;
    fn TextOutW(dc: Hdc, x: i32, y: i32, text: *const u16, length: i32) -> i32;
    fn GetTextExtentPoint32W(dc: Hdc, text: *const u16, length: i32, size: *mut Size) -> i32;
    fn CreateSolidBrush(color: u32) -> Hbrush;
    fn SaveDC(dc: Hdc) -> i32;
    fn RestoreDC(dc: Hdc, saved: i32) -> i32;
    fn IntersectClipRect(dc: Hdc, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    fn CreateRoundRectRgn(
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        ellipse_width: i32,
        ellipse_height: i32,
    ) -> Hrgn;
    fn FillRgn(dc: Hdc, region: Hrgn, brush: Hbrush) -> i32;
    fn CreateDIBSection(
        dc: Hdc,
        info: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: Handle,
        offset: u32,
    ) -> Hbitmap;
    fn CreateCompatibleDC(dc: Hdc) -> Hdc;
    fn DeleteDC(dc: Hdc) -> i32;
}

#[link(name = "msimg32")]
unsafe extern "system" {
    fn AlphaBlend(
        destination: Hdc,
        destination_x: i32,
        destination_y: i32,
        destination_width: i32,
        destination_height: i32,
        source: Hdc,
        source_x: i32,
        source_y: i32,
        source_width: i32,
        source_height: i32,
        blend: BlendFunction,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(module_name: *const u16) -> Hinstance;
    fn GetCurrentProcess() -> Handle;
    fn GetProcessTimes(
        process: Handle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn GetProcessHandleCount(process: Handle, count: *mut u32) -> i32;
}

#[link(name = "psapi")]
unsafe extern "system" {
    fn GetProcessMemoryInfo(
        process: Handle,
        counters: *mut ProcessMemoryCountersEx,
        size: u32,
    ) -> i32;
}

pub fn run() -> Result<(), String> {
    unsafe {
        let process_started = Instant::now();
        let instance = GetModuleHandleW(null());
        if instance.is_null() {
            return Err(last_error("locate application module"));
        }
        register_class(instance, MAIN_CLASS, main_window_proc, COLOR_WINDOW)?;
        register_class(instance, TASK_CLASS, task_window_proc, COLOR_WINDOW)?;

        let options = LaunchOptions::parse(process_started)?;
        let metrics = Arc::new(BrowserMetrics::default());
        let state = Box::new(BrowserState::new(instance, metrics, options));
        let state_pointer = Box::into_raw(state);
        let class = wide(MAIN_CLASS);
        let title = wide(PRODUCT_NAME);
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_VSCROLL | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1120,
            780,
            null_mut(),
            null_mut(),
            instance,
            state_pointer.cast(),
        );
        if window.is_null() {
            return Err(last_error("create browser window"));
        }

        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
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

struct LaunchOptions {
    startup_url: Option<String>,
    open_task_manager: bool,
    benchmark: Option<BenchmarkRun>,
}

impl LaunchOptions {
    fn parse(process_started: Instant) -> Result<Self, String> {
        let mut arguments = std::env::args().skip(1);
        let mut startup_url = None;
        let mut open_task_manager = false;
        let mut benchmark_url = None;
        let mut output = None;
        let mut settle_ms = 2_000_u64;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--benchmark" => {
                    benchmark_url = Some(
                        arguments
                            .next()
                            .ok_or_else(|| "--benchmark requires a URL".to_string())?,
                    );
                }
                "--output" => {
                    output = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or_else(|| "--output requires a path".to_string())?,
                    ));
                }
                "--settle-ms" => {
                    settle_ms = arguments
                        .next()
                        .ok_or_else(|| "--settle-ms requires a number".to_string())?
                        .parse::<u64>()
                        .map_err(|_| "--settle-ms must be a number".to_string())?
                        .clamp(100, 60_000);
                }
                "--task-manager" => open_task_manager = true,
                option if option.starts_with('-') => {
                    return Err(format!("unknown option: {option}"));
                }
                url => startup_url = Some(url.to_string()),
            }
        }

        let benchmark = if let Some(url) = benchmark_url {
            let output = output
                .ok_or_else(|| "benchmark mode requires --output <result.json>".to_string())?;
            startup_url = Some(url.clone());
            Some(BenchmarkRun {
                requested_url: url,
                output,
                settle: Duration::from_millis(settle_ms),
                process_started,
                initial_cpu_ticks: process_cpu_ticks().unwrap_or(0),
                window_ready: Duration::ZERO,
                navigation_started: None,
                page_ready: Duration::ZERO,
                network_time: Duration::ZERO,
                parse_time: Duration::ZERO,
                status: 0,
                bytes: 0,
                final_url: String::new(),
                error: None,
                finish_scheduled: false,
            })
        } else {
            None
        };

        Ok(Self {
            startup_url,
            open_task_manager,
            benchmark,
        })
    }
}

struct BenchmarkRun {
    requested_url: String,
    output: PathBuf,
    settle: Duration,
    process_started: Instant,
    initial_cpu_ticks: u64,
    window_ready: Duration,
    navigation_started: Option<Instant>,
    page_ready: Duration,
    network_time: Duration,
    parse_time: Duration,
    status: u32,
    bytes: u64,
    final_url: String,
    error: Option<String>,
    finish_scheduled: bool,
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

#[derive(Clone, Copy)]
enum FontKind {
    Body,
    Small,
    Heading1,
    Heading2,
    Heading3,
    Mono,
}

struct Fonts {
    body: Hfont,
    small: Hfont,
    heading1: Hfont,
    heading2: Hfont,
    heading3: Hfont,
    mono: Hfont,
}

impl Fonts {
    unsafe fn create() -> Result<Self, String> {
        let fonts = Self {
            body: create_font(-19, 400, false, "Segoe UI"),
            small: create_font(-16, 400, false, "Segoe UI"),
            heading1: create_font(-34, 600, false, "Segoe UI"),
            heading2: create_font(-28, 600, false, "Segoe UI"),
            heading3: create_font(-23, 600, false, "Segoe UI"),
            mono: create_font(-18, 400, false, "Cascadia Mono"),
        };
        if [
            fonts.body,
            fonts.small,
            fonts.heading1,
            fonts.heading2,
            fonts.heading3,
            fonts.mono,
        ]
        .iter()
        .any(|font| font.is_null())
        {
            Err(last_error("create interface fonts"))
        } else {
            Ok(fonts)
        }
    }

    fn get(&self, kind: FontKind) -> Hfont {
        match kind {
            FontKind::Body => self.body,
            FontKind::Small => self.small,
            FontKind::Heading1 => self.heading1,
            FontKind::Heading2 => self.heading2,
            FontKind::Heading3 => self.heading3,
            FontKind::Mono => self.mono,
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            for font in [
                self.body,
                self.small,
                self.heading1,
                self.heading2,
                self.heading3,
                self.mono,
            ] {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
        }
    }
}

struct Controls {
    back: Hwnd,
    forward: Hwnd,
    reload: Hwnd,
    address: Hwnd,
    go: Hwnd,
    task_manager: Hwnd,
    reader: Hwnd,
    status: Hwnd,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            back: null_mut(),
            forward: null_mut(),
            reload: null_mut(),
            address: null_mut(),
            go: null_mut(),
            task_manager: null_mut(),
            reader: null_mut(),
            status: null_mut(),
        }
    }
}

struct DrawItem {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    text: String,
    link: Option<String>,
    font: FontKind,
    color: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    family: String,
    size: i32,
    weight: u16,
    italic: bool,
    underline: bool,
}

#[derive(Default)]
struct DynamicFonts {
    fonts: HashMap<FontKey, Hfont>,
}

impl DynamicFonts {
    unsafe fn get_or_create(&mut self, spec: &FontSpec) -> Hfont {
        let key = font_key(spec);
        if let Some(font) = self.fonts.get(&key) {
            return *font;
        }
        let family = wide(&key.family);
        let font = CreateFontW(
            -key.size,
            0,
            0,
            0,
            i32::from(key.weight),
            key.italic as u32,
            key.underline as u32,
            0,
            1,
            0,
            0,
            5,
            0,
            family.as_ptr(),
        );
        self.fonts.insert(key, font);
        font
    }
}

fn font_key(spec: &FontSpec) -> FontKey {
    let requested = spec
        .family
        .split(',')
        .next()
        .unwrap_or("sans-serif")
        .trim()
        .trim_matches(['\'', '"']);
    let family = match requested.to_ascii_lowercase().as_str() {
        "sans-serif" | "system-ui" | "ui-sans-serif" => "Arial".to_string(),
        "serif" | "ui-serif" => "Times New Roman".to_string(),
        "monospace" | "ui-monospace" => "Consolas".to_string(),
        _ => requested.to_string(),
    };
    FontKey {
        family,
        size: spec.size.round().clamp(1.0, 512.0) as i32,
        weight: spec.weight.clamp(100, 900),
        italic: spec.italic,
        underline: spec.underline,
    }
}

impl Drop for DynamicFonts {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts.values().copied() {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
        }
    }
}

#[derive(Default)]
struct ImageBitmaps {
    bitmaps: HashMap<String, Hbitmap>,
}

impl ImageBitmaps {
    unsafe fn get_or_create(&mut self, key: &str, image: &DecodedImage, dc: Hdc) -> Hbitmap {
        if let Some(bitmap) = self.bitmaps.get(key) {
            return *bitmap;
        }
        let info = bitmap_info(image);
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        if !bitmap.is_null() && !pixels.is_null() {
            std::ptr::copy_nonoverlapping(image.bgra.as_ptr(), pixels.cast(), image.bgra.len());
            self.bitmaps.insert(key.to_string(), bitmap);
        }
        bitmap
    }

    unsafe fn clear(&mut self) {
        for bitmap in self.bitmaps.drain().map(|(_, bitmap)| bitmap) {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
        }
    }
}

impl Drop for ImageBitmaps {
    fn drop(&mut self) {
        unsafe { self.clear() }
    }
}

struct GdiTextMeasurer<'a> {
    dc: Hdc,
    fonts: &'a mut DynamicFonts,
}

impl TextMeasurer for GdiTextMeasurer<'_> {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        unsafe {
            let handle = self.fonts.get_or_create(font);
            SelectObject(self.dc, handle);
            let size = measure_text(self.dc, text);
            (size.cx as f32, size.cy as f32)
        }
    }
}

struct PageControlWindow {
    window: Hwnd,
    spec: better_web_browser::engine::ControlSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Surface {
    Page,
    Reader,
}

struct BrowserState {
    instance: Hinstance,
    window: Hwnd,
    controls: Controls,
    fonts: Option<Fonts>,
    dynamic_fonts: DynamicFonts,
    image_bitmaps: ImageBitmaps,
    content_brush: Hbrush,
    page: Page,
    document: Document,
    draw_items: Vec<DrawItem>,
    page_layout: LayoutOutput,
    page_controls: Vec<PageControlWindow>,
    surface: Surface,
    content_height: i32,
    scroll_y: i32,
    history: Vec<String>,
    history_index: usize,
    generation: u64,
    loading: bool,
    startup_url: Option<String>,
    open_task_manager_on_start: bool,
    benchmark: Option<BenchmarkRun>,
    metrics: Arc<BrowserMetrics>,
    task_window: Hwnd,
}

impl BrowserState {
    fn new(instance: Hinstance, metrics: Arc<BrowserMetrics>, options: LaunchOptions) -> Self {
        let home = parse_html(HOME_HTML, HOME_URL);
        let page = Page::parse(HOME_HTML, HOME_URL);
        Self {
            instance,
            window: null_mut(),
            controls: Controls::default(),
            fonts: None,
            dynamic_fonts: DynamicFonts::default(),
            image_bitmaps: ImageBitmaps::default(),
            content_brush: unsafe { CreateSolidBrush(rgb(250, 250, 248)) },
            page,
            document: home,
            draw_items: Vec::new(),
            page_layout: LayoutOutput::default(),
            page_controls: Vec::new(),
            surface: Surface::Page,
            content_height: 0,
            scroll_y: 0,
            history: Vec::new(),
            history_index: 0,
            generation: 0,
            loading: false,
            startup_url: options.startup_url,
            open_task_manager_on_start: options.open_task_manager,
            benchmark: options.benchmark,
            metrics,
            task_window: null_mut(),
        }
    }

    unsafe fn create_controls(&mut self) -> Result<(), String> {
        self.fonts = Some(Fonts::create()?);
        self.controls.back = self.create_control("BUTTON", "<", BS_PUSHBUTTON, ID_BACK);
        self.controls.forward = self.create_control("BUTTON", ">", BS_PUSHBUTTON, ID_FORWARD);
        self.controls.reload = self.create_control("BUTTON", "Reload", BS_PUSHBUTTON, ID_RELOAD);
        self.controls.address = self.create_control(
            "EDIT",
            "",
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
            ID_ADDRESS,
        );
        self.controls.go = self.create_control("BUTTON", "Go", BS_PUSHBUTTON, ID_GO);
        self.controls.task_manager =
            self.create_control("BUTTON", "Task manager", BS_PUSHBUTTON, ID_TASK_MANAGER);
        self.controls.reader = self.create_control("BUTTON", "Reader", BS_PUSHBUTTON, ID_READER);
        self.controls.status = self.create_control("STATIC", "Ready", SS_LEFT, 0);

        let all = [
            self.controls.back,
            self.controls.forward,
            self.controls.reload,
            self.controls.address,
            self.controls.go,
            self.controls.task_manager,
            self.controls.reader,
            self.controls.status,
        ];
        if all.iter().any(|window| window.is_null()) {
            return Err(last_error("create browser controls"));
        }
        let font = self.fonts.as_ref().unwrap().body;
        for control in all {
            SendMessageW(control, WM_SETFONT, font as usize, 1);
        }
        self.update_history_buttons();
        self.resize_controls();
        self.rebuild_layout();

        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.window_ready = benchmark.process_started.elapsed();
        }
        if self.open_task_manager_on_start {
            self.open_task_manager();
        }

        if let Some(url) = self.startup_url.take() {
            self.navigate_from_input(&url, HistoryMode::Push);
        } else {
            SetFocus(self.controls.address);
        }
        Ok(())
    }

    unsafe fn create_control(&self, class: &str, text: &str, extra_style: u32, id: usize) -> Hwnd {
        let class = wide(class);
        let text = wide(text);
        CreateWindowExW(
            0,
            class.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | extra_style,
            0,
            0,
            0,
            0,
            self.window,
            id as Hmenu,
            self.instance,
            null_mut(),
        )
    }

    unsafe fn resize_controls(&mut self) {
        let mut rectangle: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut rectangle);
        let width = rectangle.right.max(420);
        let height = rectangle.bottom.max(200);
        MoveWindow(self.controls.back, 8, 9, 36, 31, 1);
        MoveWindow(self.controls.forward, 48, 9, 36, 31, 1);
        MoveWindow(self.controls.reload, 88, 9, 58, 31, 1);
        MoveWindow(self.controls.address, 152, 9, (width - 416).max(50), 31, 1);
        MoveWindow(self.controls.go, width - 258, 9, 48, 31, 1);
        MoveWindow(self.controls.reader, width - 204, 9, 70, 31, 1);
        MoveWindow(self.controls.task_manager, width - 128, 9, 120, 31, 1);
        MoveWindow(
            self.controls.status,
            8,
            height - STATUS_HEIGHT + 3,
            width - 16,
            STATUS_HEIGHT - 3,
            1,
        );
    }

    unsafe fn navigate_from_address(&mut self) {
        let input = window_text(self.controls.address);
        self.navigate_from_input(&input, HistoryMode::Push);
    }

    unsafe fn navigate_from_input(&mut self, input: &str, history_mode: HistoryMode) {
        match normalize_user_input(input) {
            Ok(url) => self.begin_navigation(url, history_mode),
            Err(error) => self.set_status(&error.to_string()),
        }
    }

    unsafe fn begin_navigation(&mut self, url: String, history_mode: HistoryMode) {
        if self.loading {
            self.generation = self.generation.wrapping_add(1);
        }
        match history_mode {
            HistoryMode::Push => {
                if self.history.get(self.history_index) != Some(&url) {
                    if !self.history.is_empty() {
                        self.history.truncate(self.history_index + 1);
                    }
                    self.history.push(url.clone());
                    self.history_index = self.history.len() - 1;
                }
            }
            HistoryMode::Existing => {}
        }
        self.update_history_buttons();
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.loading = true;
        if let Some(benchmark) = self.benchmark.as_mut()
            && benchmark.navigation_started.is_none()
        {
            benchmark.navigation_started = Some(Instant::now());
        }
        set_window_text(self.controls.address, &url);
        self.set_status(&format!("Loading {url} …"));

        let window_value = self.window as isize;
        let metrics = Arc::clone(&self.metrics);
        std::thread::spawn(move || {
            let _request = metrics.begin_request();
            let started = Instant::now();
            let result = winhttp::get(&url).map(|response| {
                let mut network_time = started.elapsed();
                let mut bytes = response.body.len() as u64;
                let html = winhttp::decode_text(&response.body, response.content_type.as_deref());
                let document = parse_html(&html, &response.final_url);
                let mut rendered_page = Page::parse(&html, &response.final_url);
                let resources = rendered_page.resources.clone();
                let mut resource_budget = 32_u64 * 1024 * 1024;
                for resource in resources {
                    if resource_budget == 0 {
                        break;
                    }
                    match resource {
                        PageResource::Stylesheet { url } => {
                            let resource_started = Instant::now();
                            if let Ok(resource_response) = winhttp::get(&url) {
                                network_time += resource_started.elapsed();
                                let size = resource_response.body.len() as u64;
                                if size <= resource_budget {
                                    let css = winhttp::decode_text(
                                        &resource_response.body,
                                        resource_response.content_type.as_deref(),
                                    );
                                    rendered_page.add_stylesheet(css);
                                    bytes += size;
                                    resource_budget -= size;
                                }
                            } else {
                                network_time += resource_started.elapsed();
                            }
                        }
                        PageResource::Image { url } => {
                            let resource_started = Instant::now();
                            if let Ok(resource_response) = winhttp::get(&url) {
                                network_time += resource_started.elapsed();
                                let size = resource_response.body.len() as u64;
                                if size <= resource_budget
                                    && rendered_page
                                        .add_image(url, &resource_response.body)
                                        .is_ok()
                                {
                                    bytes += size;
                                    resource_budget -= size;
                                }
                            } else {
                                network_time += resource_started.elapsed();
                            }
                        }
                    }
                }
                let parse_time = started.elapsed().saturating_sub(network_time);
                metrics.record_success(bytes, parse_time.as_micros() as u64);
                LoadedPage {
                    page: rendered_page,
                    document,
                    final_url: response.final_url,
                    status: response.status,
                    bytes,
                    network_time,
                    parse_time,
                }
            });
            if result.is_err() {
                metrics.record_failure();
            }
            let message = Box::new(LoadMessage { generation, result });
            let pointer = Box::into_raw(message);
            let window = window_value as Hwnd;
            if unsafe { PostMessageW(window, WM_APP_PAGE_LOADED, 0, pointer as isize) } == 0 {
                unsafe {
                    drop(Box::from_raw(pointer));
                }
            }
        });
    }

    unsafe fn finish_navigation(&mut self, message: LoadMessage) {
        if message.generation != self.generation {
            return;
        }
        self.loading = false;
        match message.result {
            Ok(page) => {
                self.destroy_page_controls();
                self.image_bitmaps.clear();
                self.page = page.page;
                self.document = page.document;
                self.surface = Surface::Page;
                set_window_text(self.controls.reader, "Reader");
                self.scroll_y = 0;
                if let Some(current) = self.history.get_mut(self.history_index) {
                    *current = page.final_url.clone();
                }
                set_window_text(self.controls.address, &page.final_url);
                set_window_text(
                    self.window,
                    &format!("{} — {PRODUCT_NAME}", self.page.title),
                );
                self.rebuild_layout();
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.network_time = page.network_time;
                    benchmark.parse_time = page.parse_time;
                    benchmark.status = page.status;
                    benchmark.bytes = page.bytes;
                    benchmark.final_url = page.final_url.clone();
                }
                self.set_status(&format!(
                    "HTTP {}  •  {}  •  network {}  •  parse {}",
                    page.status,
                    format_bytes(page.bytes),
                    format_duration(page.network_time),
                    format_duration(page.parse_time)
                ));
                InvalidateRect(self.window, null(), 0);
                UpdateWindow(self.window);
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.page_ready = benchmark.process_started.elapsed();
                }
                self.schedule_benchmark_finish();
            }
            Err(error) => {
                self.set_status(&format!("Load failed: {error}"));
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.error = Some(error);
                    benchmark.page_ready = benchmark.process_started.elapsed();
                }
                self.schedule_benchmark_finish();
            }
        }
    }

    unsafe fn schedule_benchmark_finish(&mut self) {
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        if benchmark.finish_scheduled {
            return;
        }
        benchmark.finish_scheduled = true;
        let delay = benchmark.settle;
        let window = self.window as isize;
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            unsafe {
                PostMessageW(window as Hwnd, WM_APP_BENCHMARK_FINISH, 0, 0);
            }
        });
    }

    unsafe fn finish_benchmark(&mut self) {
        let Some(benchmark) = self.benchmark.as_ref() else {
            return;
        };
        let memory = process_memory();
        let elapsed = benchmark.process_started.elapsed();
        let cpu_ticks = process_cpu_ticks()
            .unwrap_or(benchmark.initial_cpu_ticks)
            .saturating_sub(benchmark.initial_cpu_ticks);
        let cpu_seconds = cpu_ticks as f64 / 10_000_000.0;
        let processors = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1) as f64;
        let average_cpu = if elapsed.is_zero() {
            0.0
        } else {
            cpu_seconds / elapsed.as_secs_f64() / processors * 100.0
        };
        let navigation_ms = benchmark
            .navigation_started
            .map(|started| {
                benchmark
                    .page_ready
                    .saturating_sub(started.duration_since(benchmark.process_started))
            })
            .unwrap_or_default()
            .as_secs_f64()
            * 1_000.0;
        let metrics = self.metrics.snapshot();
        let json = format!(
            concat!(
                "{{\n",
                "  \"browser\": {},\n",
                "  \"requested_url\": {},\n",
                "  \"final_url\": {},\n",
                "  \"error\": {},\n",
                "  \"http_status\": {},\n",
                "  \"window_ready_ms\": {:.3},\n",
                "  \"page_ready_ms\": {:.3},\n",
                "  \"navigation_ms\": {:.3},\n",
                "  \"network_ms\": {:.3},\n",
                "  \"parse_ms\": {:.3},\n",
                "  \"settle_ms\": {},\n",
                "  \"working_set_bytes\": {},\n",
                "  \"private_bytes\": {},\n",
                "  \"peak_working_set_bytes\": {},\n",
                "  \"cpu_time_ms\": {:.3},\n",
                "  \"average_cpu_percent\": {:.3},\n",
                "  \"process_count\": 1,\n",
                "  \"downloaded_bytes\": {},\n",
                "  \"retained_draw_items\": {}\n",
                "}}\n"
            ),
            json_string(BENCHMARK_ID),
            json_string(&benchmark.requested_url),
            json_string(&benchmark.final_url),
            benchmark
                .error
                .as_deref()
                .map(json_string)
                .unwrap_or_else(|| "null".into()),
            benchmark.status,
            benchmark.window_ready.as_secs_f64() * 1_000.0,
            benchmark.page_ready.as_secs_f64() * 1_000.0,
            navigation_ms,
            benchmark.network_time.as_secs_f64() * 1_000.0,
            benchmark.parse_time.as_secs_f64() * 1_000.0,
            benchmark.settle.as_millis(),
            memory.working_set,
            memory.private_usage,
            memory.peak_working_set,
            cpu_seconds * 1_000.0,
            average_cpu,
            metrics.bytes_downloaded,
            metrics.retained_draw_items,
        );
        let write_result = benchmark
            .output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::write(&benchmark.output, json));
        if let Err(error) = write_result {
            self.set_status(&format!("Failed to write benchmark: {error}"));
        }
        DestroyWindow(self.window);
    }

    unsafe fn go_back(&mut self) {
        if self.history_index > 0 {
            self.history_index -= 1;
            let url = self.history[self.history_index].clone();
            self.begin_navigation(url, HistoryMode::Existing);
        }
    }

    unsafe fn go_forward(&mut self) {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            let url = self.history[self.history_index].clone();
            self.begin_navigation(url, HistoryMode::Existing);
        }
    }

    unsafe fn reload(&mut self) {
        if let Some(url) = self.history.get(self.history_index).cloned() {
            self.begin_navigation(url, HistoryMode::Existing);
        }
    }

    unsafe fn update_history_buttons(&self) {
        EnableWindow(self.controls.back, (self.history_index > 0) as i32);
        EnableWindow(
            self.controls.forward,
            (self.history_index + 1 < self.history.len()) as i32,
        );
        EnableWindow(self.controls.reload, (!self.history.is_empty()) as i32);
    }

    unsafe fn set_status(&self, status: &str) {
        set_window_text(self.controls.status, status);
    }

    unsafe fn rebuild_layout(&mut self) {
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let dc = GetDC(self.window);
        if dc.is_null() {
            return;
        }
        SetBkMode(dc, TRANSPARENT);
        match self.surface {
            Surface::Page => {
                let viewport_height =
                    (client.bottom - TOOLBAR_HEIGHT - STATUS_HEIGHT).max(1) as f32;
                let mut measurer = GdiTextMeasurer {
                    dc,
                    fonts: &mut self.dynamic_fonts,
                };
                self.page_layout = layout_page(
                    &self.page,
                    client.right.max(1) as f32,
                    viewport_height,
                    &mut measurer,
                );
                self.content_height = self.page_layout.content_height.ceil() as i32;
                self.metrics
                    .set_retained_draw_items(self.page_layout.items.len());
            }
            Surface::Reader => {
                let Some(fonts) = self.fonts.as_ref() else {
                    ReleaseDC(self.window, dc);
                    return;
                };
                let available = (client.right - CONTENT_MARGIN * 2).max(220);
                let reading_width = available.min(MAX_READING_WIDTH);
                let left = ((client.right - reading_width) / 2).max(CONTENT_MARGIN);
                let (items, height) =
                    layout_document(dc, fonts, &self.document, left, reading_width);
                self.draw_items = items;
                self.content_height = height;
                self.metrics.set_retained_draw_items(self.draw_items.len());
            }
        }
        ReleaseDC(self.window, dc);
        self.clamp_scroll();
        self.update_scrollbar();
        self.recreate_page_controls();
    }

    unsafe fn toggle_reader(&mut self) {
        self.surface = match self.surface {
            Surface::Page => Surface::Reader,
            Surface::Reader => Surface::Page,
        };
        set_window_text(
            self.controls.reader,
            if self.surface == Surface::Reader {
                "Page"
            } else {
                "Reader"
            },
        );
        self.scroll_y = 0;
        self.rebuild_layout();
        InvalidateRect(self.window, null(), 0);
    }

    unsafe fn destroy_page_controls(&mut self) {
        for control in self.page_controls.drain(..) {
            if !control.window.is_null() && IsWindow(control.window) != 0 {
                DestroyWindow(control.window);
            }
        }
    }

    unsafe fn recreate_page_controls(&mut self) {
        let previous_values = self
            .page_controls
            .iter()
            .filter(|control| {
                matches!(
                    control.spec.kind,
                    ControlKind::Text
                        | ControlKind::TextArea
                        | ControlKind::Password
                        | ControlKind::Search
                )
            })
            .map(|control| (control.spec.node_id, window_text(control.window)))
            .collect::<HashMap<_, _>>();
        self.destroy_page_controls();
        if self.surface != Surface::Page {
            return;
        }
        let specs = self
            .page_layout
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Control(spec) => Some(spec.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (index, spec) in specs.into_iter().enumerate() {
            let id = ID_PAGE_CONTROL_BASE + index;
            let (class, style, text) = match spec.kind {
                ControlKind::Submit | ControlKind::Button | ControlKind::Reset => {
                    ("BUTTON", BS_PUSHBUTTON | WS_TABSTOP, spec.value.clone())
                }
                ControlKind::Password => (
                    "EDIT",
                    WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL | ES_PASSWORD,
                    previous_values
                        .get(&spec.node_id)
                        .cloned()
                        .unwrap_or_else(|| spec.value.clone()),
                ),
                ControlKind::TextArea => (
                    "EDIT",
                    WS_BORDER | WS_TABSTOP | ES_MULTILINE | ES_AUTOVSCROLL,
                    previous_values
                        .get(&spec.node_id)
                        .cloned()
                        .unwrap_or_else(|| spec.value.clone()),
                ),
                _ => (
                    "EDIT",
                    WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
                    previous_values
                        .get(&spec.node_id)
                        .cloned()
                        .unwrap_or_else(|| spec.value.clone()),
                ),
            };
            let window = self.create_control(class, &text, style, id);
            if window.is_null() {
                continue;
            }
            if let Some(fonts) = self.fonts.as_ref() {
                SendMessageW(window, WM_SETFONT, fonts.body as usize, 1);
            }
            if !spec.placeholder.is_empty()
                && matches!(
                    spec.kind,
                    ControlKind::Text
                        | ControlKind::TextArea
                        | ControlKind::Password
                        | ControlKind::Search
                )
            {
                let placeholder = wide(&spec.placeholder);
                SendMessageW(window, EM_SETCUEBANNER, 1, placeholder.as_ptr() as isize);
            }
            self.page_controls.push(PageControlWindow { window, spec });
        }
        self.sync_page_control_positions();
    }

    unsafe fn sync_page_control_positions(&self) {
        let viewport_height = self.viewport_height();
        for control in &self.page_controls {
            let rect = control.spec.rect;
            let screen_y = TOOLBAR_HEIGHT + rect.y.round() as i32 - self.scroll_y;
            let visible = screen_y + rect.height.ceil() as i32 >= TOOLBAR_HEIGHT
                && screen_y <= TOOLBAR_HEIGHT + viewport_height;
            if visible {
                MoveWindow(
                    control.window,
                    rect.x.round() as i32,
                    screen_y,
                    rect.width.ceil().max(1.0) as i32,
                    rect.height.ceil().max(1.0) as i32,
                    1,
                );
                ShowWindow(control.window, SW_SHOW);
            } else {
                ShowWindow(control.window, SW_HIDE);
            }
        }
    }

    unsafe fn activate_page_control(&mut self, id: usize, notification: usize) {
        let Some(index) = id.checked_sub(ID_PAGE_CONTROL_BASE) else {
            return;
        };
        let Some(control) = self.page_controls.get(index) else {
            return;
        };
        let spec = control.spec.clone();
        let is_button = matches!(
            spec.kind,
            ControlKind::Submit | ControlKind::Button | ControlKind::Reset
        );
        if !is_button && notification != 0 {
            return;
        }
        if spec.kind == ControlKind::Button {
            self.set_status("This button requires JavaScript, which is not implemented yet.");
            return;
        }
        let Some(form_id) = spec.form_id else {
            self.set_status("This control requires JavaScript, which is not implemented yet.");
            return;
        };
        if spec.kind == ControlKind::Reset {
            for page_control in &self.page_controls {
                if page_control.spec.form_id == Some(form_id)
                    && matches!(
                        page_control.spec.kind,
                        ControlKind::Text
                            | ControlKind::TextArea
                            | ControlKind::Password
                            | ControlKind::Search
                    )
                {
                    set_window_text(page_control.window, &page_control.spec.value);
                }
            }
            return;
        }
        let Some(form) = self.page_layout.forms.get(&form_id).cloned() else {
            return;
        };
        if form.method != "get" {
            self.set_status("POST form submission is not implemented yet.");
            return;
        }
        let mut fields = form.hidden_fields;
        for page_control in &self.page_controls {
            if page_control.spec.form_id != Some(form_id) || page_control.spec.name.is_empty() {
                continue;
            }
            match page_control.spec.kind {
                ControlKind::Text
                | ControlKind::TextArea
                | ControlKind::Password
                | ControlKind::Search => fields.push((
                    page_control.spec.name.clone(),
                    window_text(page_control.window),
                )),
                ControlKind::Submit if page_control.spec.node_id == spec.node_id => {
                    fields.push((
                        page_control.spec.name.clone(),
                        page_control.spec.value.clone(),
                    ));
                }
                _ => {}
            }
        }
        let query = fields
            .iter()
            .map(|(name, value)| {
                format!(
                    "{}={}",
                    encode_www_form_component(name),
                    encode_www_form_component(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let separator = if form.action.contains('?') { '&' } else { '?' };
        let target = if query.is_empty() {
            form.action
        } else {
            format!("{}{separator}{query}", form.action)
        };
        self.begin_navigation(target, HistoryMode::Push);
    }

    unsafe fn viewport_height(&self) -> i32 {
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        (client.bottom - TOOLBAR_HEIGHT - STATUS_HEIGHT).max(1)
    }

    unsafe fn clamp_scroll(&mut self) {
        let max_scroll = (self.content_height - self.viewport_height()).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max_scroll);
    }

    unsafe fn scroll_to(&mut self, position: i32) {
        self.scroll_y = position;
        self.clamp_scroll();
        self.update_scrollbar();
        self.sync_page_control_positions();
        InvalidateRect(self.window, null(), 0);
    }

    unsafe fn update_scrollbar(&self) {
        let info = ScrollInfo {
            size: size_of::<ScrollInfo>() as u32,
            mask: SIF_RANGE | SIF_PAGE | SIF_POS,
            min: 0,
            max: self.content_height.saturating_sub(1),
            page: self.viewport_height() as u32,
            position: self.scroll_y,
            track_position: 0,
        };
        SetScrollInfo(self.window, SB_VERT, &info, 1);
    }

    unsafe fn handle_scroll(&mut self, command: u16) {
        let viewport = self.viewport_height();
        let target = match command {
            SB_LINEUP => self.scroll_y - 42,
            SB_LINEDOWN => self.scroll_y + 42,
            SB_PAGEUP => self.scroll_y - viewport,
            SB_PAGEDOWN => self.scroll_y + viewport,
            SB_TOP => 0,
            SB_BOTTOM => self.content_height,
            SB_THUMBPOSITION | SB_THUMBTRACK => {
                let mut info = ScrollInfo {
                    size: size_of::<ScrollInfo>() as u32,
                    mask: SIF_TRACKPOS,
                    min: 0,
                    max: 0,
                    page: 0,
                    position: 0,
                    track_position: 0,
                };
                GetScrollInfo(self.window, SB_VERT, &mut info);
                info.track_position
            }
            _ => self.scroll_y,
        };
        self.scroll_to(target);
    }

    unsafe fn click_content(&mut self, x: i32, y: i32) {
        if y < TOOLBAR_HEIGHT || y > TOOLBAR_HEIGHT + self.viewport_height() {
            return;
        }
        let document_y = y - TOOLBAR_HEIGHT + self.scroll_y;
        let url = match self.surface {
            Surface::Page => self.page_layout.items.iter().find_map(|item| match item {
                DisplayItem::Text {
                    rect,
                    link: Some(link),
                    ..
                } if x as f32 >= rect.x
                    && x as f32 <= rect.right()
                    && document_y as f32 >= rect.y
                    && document_y as f32 <= rect.bottom() =>
                {
                    Some(link.clone())
                }
                _ => None,
            }),
            Surface::Reader => self
                .draw_items
                .iter()
                .find(|item| {
                    item.link.is_some()
                        && x >= item.x
                        && x <= item.x + item.width
                        && document_y >= item.y
                        && document_y <= item.y + item.height
                })
                .and_then(|item| item.link.clone()),
        };
        if let Some(url) = url {
            self.begin_navigation(url, HistoryMode::Push);
        }
    }

    unsafe fn paint(&mut self) {
        let mut paint: PaintStruct = std::mem::zeroed();
        let dc = BeginPaint(self.window, &mut paint);
        if dc.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let content = Rect {
            left: 0,
            top: TOOLBAR_HEIGHT,
            right: client.right,
            bottom: (client.bottom - STATUS_HEIGHT).max(TOOLBAR_HEIGHT),
        };
        match self.surface {
            Surface::Page => {
                fill_color_rect(dc, &content, self.page_layout.background.to_colorref())
            }
            Surface::Reader => {
                FillRect(dc, &content, self.content_brush);
            }
        }
        SetBkMode(dc, TRANSPARENT);
        let saved_dc = SaveDC(dc);
        IntersectClipRect(dc, content.left, content.top, content.right, content.bottom);
        match self.surface {
            Surface::Page => {
                for item in &self.page_layout.items {
                    match item {
                        DisplayItem::SolidRect {
                            rect,
                            color,
                            radius,
                        } => {
                            let rectangle = screen_rect(*rect, self.scroll_y);
                            if intersects(&rectangle, &content) {
                                fill_color_shape(dc, &rectangle, color.to_colorref(), *radius);
                            }
                        }
                        DisplayItem::BorderRect {
                            rect,
                            widths,
                            color,
                            ..
                        } => {
                            let rectangle = screen_rect(*rect, self.scroll_y);
                            if intersects(&rectangle, &content) {
                                paint_border(dc, &rectangle, *widths, color.to_colorref());
                            }
                        }
                        DisplayItem::Text {
                            rect,
                            text,
                            font,
                            color,
                            ..
                        } => {
                            let screen_y = TOOLBAR_HEIGHT + rect.y.round() as i32 - self.scroll_y;
                            if screen_y + (rect.height.ceil() as i32) < content.top
                                || screen_y > content.bottom
                            {
                                continue;
                            }
                            let font_handle = self.dynamic_fonts.get_or_create(font);
                            SelectObject(dc, font_handle);
                            SetTextColor(dc, color.to_colorref());
                            let text = wide_without_null(text);
                            TextOutW(
                                dc,
                                rect.x.round() as i32,
                                screen_y,
                                text.as_ptr(),
                                text.len() as i32,
                            );
                        }
                        DisplayItem::Image { rect, url, alt } => {
                            let screen_y = TOOLBAR_HEIGHT + rect.y.round() as i32 - self.scroll_y;
                            if screen_y + (rect.height.ceil() as i32) < content.top
                                || screen_y > content.bottom
                            {
                                continue;
                            }
                            if let Some(image) = self.page.images.get(url) {
                                let bitmap = self.image_bitmaps.get_or_create(url, image, dc);
                                if !bitmap.is_null() {
                                    paint_alpha_image(dc, bitmap, image, *rect, screen_y);
                                }
                            } else if !alt.is_empty()
                                && let Some(fonts) = self.fonts.as_ref()
                            {
                                SelectObject(dc, fonts.body);
                                SetTextColor(dc, rgb(70, 70, 70));
                                let alt = wide_without_null(alt);
                                TextOutW(
                                    dc,
                                    rect.x.round() as i32,
                                    screen_y,
                                    alt.as_ptr(),
                                    alt.len() as i32,
                                );
                            }
                        }
                        DisplayItem::Control(_) => {}
                    }
                }
            }
            Surface::Reader => {
                if let Some(fonts) = self.fonts.as_ref() {
                    for item in &self.draw_items {
                        let screen_y = TOOLBAR_HEIGHT + item.y - self.scroll_y;
                        if screen_y + item.height < content.top || screen_y > content.bottom {
                            continue;
                        }
                        SelectObject(dc, fonts.get(item.font));
                        SetTextColor(dc, item.color);
                        let text = wide_without_null(&item.text);
                        TextOutW(dc, item.x, screen_y, text.as_ptr(), text.len() as i32);
                    }
                }
            }
        }
        if saved_dc != 0 {
            RestoreDC(dc, saved_dc);
        }
        EndPaint(self.window, &paint);
    }

    unsafe fn open_task_manager(&mut self) {
        if !self.task_window.is_null() && IsWindow(self.task_window) != 0 {
            SetForegroundWindow(self.task_window);
            return;
        }
        let state = Box::new(TaskManagerState::new(
            self.window,
            Arc::clone(&self.metrics),
        ));
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
            430,
            390,
            self.window,
            null_mut(),
            self.instance,
            pointer.cast(),
        );
        if window.is_null() {
            self.set_status(&last_error("open task manager"));
        } else {
            self.task_window = window;
            ShowWindow(window, SW_SHOW);
            UpdateWindow(window);
        }
    }
}

impl Drop for BrowserState {
    fn drop(&mut self) {
        unsafe {
            if !self.content_brush.is_null() {
                DeleteObject(self.content_brush);
            }
        }
    }
}

enum HistoryMode {
    Push,
    Existing,
}

struct LoadedPage {
    page: Page,
    document: Document,
    final_url: String,
    status: u32,
    bytes: u64,
    network_time: Duration,
    parse_time: Duration,
}

struct LoadMessage {
    generation: u64,
    result: Result<LoadedPage, String>,
}

unsafe extern "system" fn main_window_proc(
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
        WM_SIZE => {
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
        WM_APP_PAGE_LOADED => {
            let message = Box::from_raw(lparam as *mut LoadMessage);
            state.finish_navigation(*message);
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
            state.click_content(x, y);
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

fn layout_document(
    dc: Hdc,
    fonts: &Fonts,
    document: &Document,
    left: i32,
    width: i32,
) -> (Vec<DrawItem>, i32) {
    let mut items = Vec::new();
    let mut y = 28;
    unsafe {
        layout_spans(
            dc,
            fonts,
            &mut items,
            &[Span {
                text: document.title.clone(),
                link: None,
            }],
            FontKind::Heading1,
            left,
            width,
            &mut y,
            43,
            rgb(30, 34, 40),
            "",
        );
        y += 2;
        layout_spans(
            dc,
            fonts,
            &mut items,
            &[Span {
                text: document.source_url.clone(),
                link: Some(document.source_url.clone()),
            }],
            FontKind::Small,
            left,
            width,
            &mut y,
            22,
            rgb(38, 102, 180),
            "",
        );
        y += 25;

        for block in &document.blocks {
            let (font, line_height, color, indent, prefix, spacing) = match block.kind {
                BlockKind::Heading(1) => (FontKind::Heading2, 36, rgb(35, 39, 46), 0, "", 18),
                BlockKind::Heading(2) => (FontKind::Heading2, 36, rgb(35, 39, 46), 0, "", 16),
                BlockKind::Heading(_) => (FontKind::Heading3, 31, rgb(43, 47, 54), 0, "", 13),
                BlockKind::ListItem => (FontKind::Body, 28, rgb(42, 44, 48), 22, "• ", 5),
                BlockKind::Quote => (FontKind::Body, 29, rgb(80, 82, 86), 30, "“ ", 12),
                BlockKind::Preformatted => (FontKind::Mono, 26, rgb(48, 50, 53), 18, "", 13),
                BlockKind::Paragraph => (FontKind::Body, 29, rgb(42, 44, 48), 0, "", 12),
            };
            y += spacing;
            layout_spans(
                dc,
                fonts,
                &mut items,
                &block.spans,
                font,
                left + indent,
                width - indent,
                &mut y,
                line_height,
                color,
                prefix,
            );
        }

        if document.truncated {
            y += 18;
            layout_spans(
                dc,
                fonts,
                &mut items,
                &[Span {
                    text: "Document text was truncated at the 2 MiB safety limit.".into(),
                    link: None,
                }],
                FontKind::Small,
                left,
                width,
                &mut y,
                22,
                rgb(160, 70, 35),
                "",
            );
        }
    }
    (items, y + 48)
}

#[allow(clippy::too_many_arguments)]
unsafe fn layout_spans(
    dc: Hdc,
    fonts: &Fonts,
    output: &mut Vec<DrawItem>,
    spans: &[Span],
    font: FontKind,
    left: i32,
    width: i32,
    y: &mut i32,
    line_height: i32,
    color: u32,
    prefix: &str,
) {
    SelectObject(dc, fonts.get(font));
    let right = left + width;
    let mut x = left;
    let mut line_has_text = false;

    if !prefix.is_empty() {
        let prefix_width = measure_text(dc, prefix).cx;
        output.push(DrawItem {
            x,
            y: *y,
            width: prefix_width,
            height: line_height,
            text: prefix.to_string(),
            link: None,
            font,
            color,
        });
        x += prefix_width;
        line_has_text = true;
    }

    let mut pending_space = false;
    for span in spans {
        for (word, preceded_by_space) in words_with_spacing(&span.text) {
            let needs_space = line_has_text && (pending_space || preceded_by_space);
            let display = if needs_space {
                format!(" {word}")
            } else {
                word.to_string()
            };
            let mut item_width = measure_text(dc, &display).cx;
            if x + item_width > right && line_has_text {
                *y += line_height;
                x = left;
                let display_without_space = word.to_string();
                item_width = measure_text(dc, &display_without_space).cx;
                output.push(DrawItem {
                    x,
                    y: *y,
                    width: item_width,
                    height: line_height,
                    text: display_without_space,
                    link: span.link.clone(),
                    font,
                    color: if span.link.is_some() {
                        rgb(38, 102, 180)
                    } else {
                        color
                    },
                });
            } else {
                output.push(DrawItem {
                    x,
                    y: *y,
                    width: item_width,
                    height: line_height,
                    text: display,
                    link: span.link.clone(),
                    font,
                    color: if span.link.is_some() {
                        rgb(38, 102, 180)
                    } else {
                        color
                    },
                });
            }
            x += item_width;
            line_has_text = true;
            pending_space = false;
        }
        pending_space = span.text.chars().last().is_some_and(char::is_whitespace);
    }
    *y += line_height;
}

fn words_with_spacing(text: &str) -> Vec<(&str, bool)> {
    let mut words = Vec::new();
    let mut word_start = None;
    let mut whitespace_before_word = false;
    let mut saw_whitespace = false;
    for (index, character) in text.char_indices() {
        if character.is_whitespace() {
            if let Some(start) = word_start.take() {
                words.push((&text[start..index], whitespace_before_word));
            }
            saw_whitespace = true;
        } else if word_start.is_none() {
            word_start = Some(index);
            whitespace_before_word = saw_whitespace;
            saw_whitespace = false;
        }
    }
    if let Some(start) = word_start {
        words.push((&text[start..], whitespace_before_word));
    }
    words
}

struct TaskManagerState {
    parent: Hwnd,
    window: Hwnd,
    label: Hwnd,
    font: Hfont,
    metrics: Arc<BrowserMetrics>,
    started: Instant,
    previous_sample: Instant,
    previous_cpu_ticks: u64,
    cpu_percent: f64,
}

impl TaskManagerState {
    fn new(parent: Hwnd, metrics: Arc<BrowserMetrics>) -> Self {
        Self {
            parent,
            window: null_mut(),
            label: null_mut(),
            font: null_mut(),
            metrics,
            started: Instant::now(),
            previous_sample: Instant::now(),
            previous_cpu_ticks: process_cpu_ticks().unwrap_or(0),
            cpu_percent: 0.0,
        }
    }

    unsafe fn create(&mut self, instance: Hinstance) -> Result<(), String> {
        self.font = create_font(-18, 400, false, "Segoe UI");
        let class = wide("STATIC");
        let empty = wide("");
        self.label = CreateWindowExW(
            0,
            class.as_ptr(),
            empty.as_ptr(),
            WS_CHILD | WS_VISIBLE | SS_LEFT,
            18,
            16,
            380,
            320,
            self.window,
            null_mut(),
            instance,
            null_mut(),
        );
        if self.label.is_null() || self.font.is_null() {
            return Err(last_error("create task manager controls"));
        }
        SendMessageW(self.label, WM_SETFONT, self.font as usize, 1);
        SetTimer(self.window, 1, 1_000, null());
        self.refresh();
        Ok(())
    }

    unsafe fn refresh(&mut self) {
        let now = Instant::now();
        if let Some(current_ticks) = process_cpu_ticks() {
            let elapsed = now.duration_since(self.previous_sample).as_secs_f64();
            if elapsed > 0.0 {
                let cpu_seconds =
                    current_ticks.saturating_sub(self.previous_cpu_ticks) as f64 / 10_000_000.0;
                let processors = std::thread::available_parallelism()
                    .map(|count| count.get())
                    .unwrap_or(1) as f64;
                self.cpu_percent = (cpu_seconds / elapsed / processors * 100.0).clamp(0.0, 100.0);
            }
            self.previous_cpu_ticks = current_ticks;
        }
        self.previous_sample = now;

        let memory = process_memory();
        let snapshot = self.metrics.snapshot();
        let mut handles = 0_u32;
        GetProcessHandleCount(GetCurrentProcess(), &mut handles);
        let text = format!(
            "BROWSER PROCESS\r\n\r\nCPU usage\t\t{:>6.2}%\r\nWorking set\t\t{}\r\nPrivate memory\t\t{}\r\nPeak working set\t{}\r\nProcess handles\t\t{}\r\nUptime\t\t\t{}\r\n\r\nFAST DOCUMENT PATH\r\n\r\nActive requests\t\t{}\r\nPages completed\t\t{}\r\nFailed loads\t\t{}\r\nDownloaded\t\t{}\r\nLast HTML parse\t\t{}\r\nRetained draw items\t{}",
            self.cpu_percent,
            format_bytes(memory.working_set as u64),
            format_bytes(memory.private_usage as u64),
            format_bytes(memory.peak_working_set as u64),
            handles,
            format_duration(self.started.elapsed()),
            snapshot.active_requests,
            snapshot.pages_loaded,
            snapshot.failed_loads,
            format_bytes(snapshot.bytes_downloaded),
            format_duration(Duration::from_micros(snapshot.last_parse_micros)),
            snapshot.retained_draw_items,
        );
        set_window_text(self.label, &text);
    }

    unsafe fn resize(&self) {
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        MoveWindow(
            self.label,
            18,
            16,
            (client.right - 36).max(100),
            (client.bottom - 30).max(100),
            1,
        );
    }
}

impl Drop for TaskManagerState {
    fn drop(&mut self) {
        unsafe {
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
        }
    }
}

unsafe extern "system" fn task_window_proc(
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
            let instance = (*(lparam as *const CreateStruct)).instance;
            if state.create(instance).is_err() {
                -1
            } else {
                0
            }
        }
        WM_SIZE => {
            state.resize();
            0
        }
        WM_TIMER => {
            state.refresh();
            0
        }
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

struct MemorySample {
    working_set: usize,
    peak_working_set: usize,
    private_usage: usize,
}

fn process_memory() -> MemorySample {
    unsafe {
        let mut counters: ProcessMemoryCountersEx = std::mem::zeroed();
        counters.size = size_of::<ProcessMemoryCountersEx>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.size) == 0 {
            MemorySample {
                working_set: 0,
                peak_working_set: 0,
                private_usage: 0,
            }
        } else {
            MemorySample {
                working_set: counters.working_set_size,
                peak_working_set: counters.peak_working_set_size,
                private_usage: counters.private_usage,
            }
        }
    }
}

fn process_cpu_ticks() -> Option<u64> {
    unsafe {
        let mut creation = FileTime { low: 0, high: 0 };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        if GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        ) == 0
        {
            None
        } else {
            Some(file_time_ticks(kernel) + file_time_ticks(user))
        }
    }
}

fn file_time_ticks(time: FileTime) -> u64 {
    ((time.high as u64) << 32) | time.low as u64
}

fn screen_rect(rect: RectF, scroll_y: i32) -> Rect {
    Rect {
        left: rect.x.round() as i32,
        top: TOOLBAR_HEIGHT + rect.y.round() as i32 - scroll_y,
        right: rect.right().ceil() as i32,
        bottom: TOOLBAR_HEIGHT + rect.bottom().ceil() as i32 - scroll_y,
    }
}

fn bitmap_info(image: &DecodedImage) -> BitmapInfo {
    BitmapInfo {
        header: BitmapInfoHeader {
            size: size_of::<BitmapInfoHeader>() as u32,
            width: image.width as i32,
            height: -(image.height as i32),
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: image.bgra.len() as u32,
            x_pixels_per_meter: 0,
            y_pixels_per_meter: 0,
            colors_used: 0,
            colors_important: 0,
        },
        colors: [0],
    }
}

unsafe fn paint_alpha_image(
    destination: Hdc,
    bitmap: Hbitmap,
    image: &DecodedImage,
    rect: RectF,
    screen_y: i32,
) {
    let source = CreateCompatibleDC(destination);
    if source.is_null() {
        return;
    }
    let previous = SelectObject(source, bitmap);
    AlphaBlend(
        destination,
        rect.x.round() as i32,
        screen_y,
        rect.width.round().max(1.0) as i32,
        rect.height.round().max(1.0) as i32,
        source,
        0,
        0,
        image.width as i32,
        image.height as i32,
        BlendFunction {
            operation: 0,
            flags: 0,
            source_constant_alpha: 255,
            alpha_format: 1,
        },
    );
    if !previous.is_null() {
        SelectObject(source, previous);
    }
    DeleteDC(source);
}

fn intersects(left: &Rect, right: &Rect) -> bool {
    left.left < right.right
        && left.right > right.left
        && left.top < right.bottom
        && left.bottom > right.top
}

unsafe fn fill_color_rect(dc: Hdc, rectangle: &Rect, color: u32) {
    if rectangle.right <= rectangle.left || rectangle.bottom <= rectangle.top {
        return;
    }
    let brush = CreateSolidBrush(color);
    if !brush.is_null() {
        FillRect(dc, rectangle, brush);
        DeleteObject(brush);
    }
}

unsafe fn fill_color_shape(dc: Hdc, rectangle: &Rect, color: u32, radius: f32) {
    if radius <= 0.0 {
        fill_color_rect(dc, rectangle, color);
        return;
    }
    let brush = CreateSolidBrush(color);
    if brush.is_null() {
        return;
    }
    let diameter = (radius * 2.0).round().max(1.0) as i32;
    let region = CreateRoundRectRgn(
        rectangle.left,
        rectangle.top,
        rectangle.right + 1,
        rectangle.bottom + 1,
        diameter,
        diameter,
    );
    if !region.is_null() {
        FillRgn(dc, region, brush);
        DeleteObject(region);
    }
    DeleteObject(brush);
}

unsafe fn paint_border(dc: Hdc, rectangle: &Rect, widths: [f32; 4], color: u32) {
    let [top, right, bottom, left] = widths.map(|width| width.ceil().max(0.0) as i32);
    if top > 0 {
        fill_color_rect(
            dc,
            &Rect {
                left: rectangle.left,
                top: rectangle.top,
                right: rectangle.right,
                bottom: (rectangle.top + top).min(rectangle.bottom),
            },
            color,
        );
    }
    if right > 0 {
        fill_color_rect(
            dc,
            &Rect {
                left: (rectangle.right - right).max(rectangle.left),
                top: rectangle.top,
                right: rectangle.right,
                bottom: rectangle.bottom,
            },
            color,
        );
    }
    if bottom > 0 {
        fill_color_rect(
            dc,
            &Rect {
                left: rectangle.left,
                top: (rectangle.bottom - bottom).max(rectangle.top),
                right: rectangle.right,
                bottom: rectangle.bottom,
            },
            color,
        );
    }
    if left > 0 {
        fill_color_rect(
            dc,
            &Rect {
                left: rectangle.left,
                top: rectangle.top,
                right: (rectangle.left + left).min(rectangle.right),
                bottom: rectangle.bottom,
            },
            color,
        );
    }
}

unsafe fn create_font(height: i32, weight: i32, italic: bool, face: &str) -> Hfont {
    let face = wide(face);
    CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        italic as u32,
        0,
        0,
        1,
        0,
        0,
        5,
        0,
        face.as_ptr(),
    )
}

unsafe fn measure_text(dc: Hdc, text: &str) -> Size {
    let text = wide_without_null(text);
    let mut size = Size { cx: 0, cy: 0 };
    GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32, &mut size);
    size
}

unsafe fn window_text(window: Hwnd) -> String {
    let length = GetWindowTextLengthW(window).max(0) as usize;
    let mut buffer = vec![0_u16; length + 1];
    let copied = GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied])
}

unsafe fn set_window_text(window: Hwnd, text: &str) {
    let text = wide(text);
    SetWindowTextW(window, text.as_ptr());
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn wide_without_null(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn int_resource(identifier: u16) -> *const u16 {
    identifier as usize as *const u16
}

const fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn format_duration(duration: Duration) -> String {
    if duration.as_secs() >= 60 {
        format!(
            "{}m {:02}s",
            duration.as_secs() / 60,
            duration.as_secs() % 60
        )
    } else if duration.as_secs() >= 1 {
        format!("{:.2} s", duration.as_secs_f64())
    } else if duration.as_millis() >= 1 {
        format!("{} ms", duration.as_millis())
    } else {
        format!("{} µs", duration.as_micros())
    }
}

fn last_error(operation: &str) -> String {
    format!("Failed to {operation}: {}", io::Error::last_os_error())
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character < ' ' => {
                use std::fmt::Write;
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_original_word_spacing() {
        assert_eq!(
            words_with_spacing("hello   world"),
            vec![("hello", false), ("world", true)]
        );
        assert_eq!(words_with_spacing("joined"), vec![("joined", false)]);
        assert_eq!(words_with_spacing("  leading"), vec![("leading", true)]);
    }

    #[test]
    fn escapes_json_strings_without_a_dependency() {
        assert_eq!(json_string("a\n\"b\\c"), "\"a\\n\\\"b\\\\c\"");
    }
}
