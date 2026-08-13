#![allow(unsafe_op_in_unsafe_fn)]

use better_web_browser::branding::{BENCHMARK_ID, HOME_HTML, HOME_URL, PRODUCT_NAME};
use better_web_browser::document::{BlockKind, Document, Span, parse_html};
use better_web_browser::engine::{
    ControlKind, DecodedImage, DisplayItem, FontSpec, LayoutOutput, Page, PageResource, RectF,
    ScriptOutcome, TextMeasurer, WebFont, layout_page,
};
use better_web_browser::metrics::BrowserMetrics;
use better_web_browser::navigation::{encode_www_form_component, normalize_user_input};
use better_web_browser::winhttp;
use std::collections::{HashMap, HashSet};
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
const WM_SETFOCUS: u32 = 0x0007;
const WM_KILLFOCUS: u32 = 0x0008;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_DRAWITEM: u32 = 0x002B;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const WM_VSCROLL: u32 = 0x0115;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_KEYDOWN: u32 = 0x0100;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_DPICHANGED: u32 = 0x02E0;
const WM_NCCREATE: u32 = 0x0081;
const WM_NCDESTROY: u32 = 0x0082;
const WM_SETFONT: u32 = 0x0030;
const EM_SETCUEBANNER: u32 = 0x1501;
const EM_SETMARGINS: u32 = 0x00D3;
const CB_ADDSTRING: u32 = 0x0143;
const CB_GETCURSEL: u32 = 0x0147;
const CB_SETCURSEL: u32 = 0x014E;
const WM_APP: u32 = 0x8000;
const WM_APP_PAGE_LOADED: u32 = WM_APP + 1;
const WM_APP_TASK_CLOSED: u32 = WM_APP + 2;
const WM_APP_BENCHMARK_FINISH: u32 = WM_APP + 3;
const WM_APP_CHROME_INVALIDATE: u32 = WM_APP + 4;
const WM_APP_DEFERRED_RESOURCES: u32 = WM_APP + 5;

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_PASSWORD: u32 = 0x0020;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const BS_OWNERDRAW: u32 = 0x000B;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;

const SW_SHOW: i32 = 5;
const CW_USEDEFAULT: i32 = i32::MIN;
const GWLP_USERDATA: i32 = -21;
const COLOR_WINDOW: usize = 5;
const IDC_ARROW: u16 = 32512;
const TRANSPARENT: i32 = 1;
const VK_RETURN: usize = 0x0D;
const TME_LEAVE: u32 = 0x0000_0002;

const ODS_SELECTED: u32 = 0x0001;
const ODS_DISABLED: u32 = 0x0004;
const ODS_FOCUS: u32 = 0x0010;

const DT_CENTER: u32 = 0x0000_0001;
const DT_VCENTER: u32 = 0x0000_0004;
const DT_SINGLELINE: u32 = 0x0000_0020;
const DT_END_ELLIPSIS: u32 = 0x0000_8000;
const DT_NOPREFIX: u32 = 0x0000_0800;

const SRCCOPY: u32 = 0x00CC_0020;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;

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

const DEFAULT_DPI: u32 = 96;
const DEFAULT_WINDOW_WIDTH_DIP: i32 = 1120;
const DEFAULT_WINDOW_HEIGHT_DIP: i32 = 780;
const TOOLBAR_HEIGHT_DIP: i32 = 64;
const STATUS_HEIGHT_DIP: i32 = 30;
const CONTENT_MARGIN_DIP: i32 = 28;
const MAX_READING_WIDTH_DIP: i32 = 920;
const SW_HIDE: i32 = 0;
const DIB_RGB_COLORS: u32 = 0;
const RGN_DIFF: i32 = 4;

#[derive(Clone, Copy)]
struct ChromeTheme {
    toolbar: u32,
    status: u32,
    border: u32,
    field: u32,
    text: u32,
    muted_text: u32,
    disabled_text: u32,
    hover: u32,
    pressed: u32,
    accent: u32,
    accent_hover: u32,
    accent_pressed: u32,
    accent_soft: u32,
    focus: u32,
    card: u32,
    task_background: u32,
    success: u32,
}

const CHROME_THEME: ChromeTheme = ChromeTheme {
    toolbar: rgb(247, 249, 252),
    status: rgb(249, 250, 252),
    border: rgb(216, 222, 230),
    field: rgb(255, 255, 255),
    text: rgb(31, 41, 55),
    muted_text: rgb(96, 108, 125),
    disabled_text: rgb(168, 177, 190),
    hover: rgb(232, 236, 242),
    pressed: rgb(218, 224, 233),
    accent: rgb(20, 96, 214),
    accent_hover: rgb(16, 82, 190),
    accent_pressed: rgb(13, 67, 158),
    accent_soft: rgb(228, 238, 255),
    focus: rgb(91, 149, 240),
    card: rgb(255, 255, 255),
    task_background: rgb(244, 247, 251),
    success: rgb(31, 157, 99),
};

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Rect {
    fn width(&self) -> i32 {
        (self.right - self.left).max(0)
    }

    fn height(&self) -> i32 {
        (self.bottom - self.top).max(0)
    }

    fn inset(self, horizontal: i32, vertical: i32) -> Self {
        Self {
            left: self.left + horizontal,
            top: self.top + vertical,
            right: (self.right - horizontal).max(self.left + horizontal),
            bottom: (self.bottom - vertical).max(self.top + vertical),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
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
type SubclassProc = unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam, usize, usize) -> Lresult;

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
struct DrawItemStruct {
    control_type: u32,
    control_id: u32,
    item_id: u32,
    item_action: u32,
    item_state: u32,
    item_window: Hwnd,
    dc: Hdc,
    item_rect: Rect,
    item_data: usize,
}

#[repr(C)]
struct TrackMouseEventData {
    size: u32,
    flags: u32,
    track_window: Hwnd,
    hover_time: u32,
}

#[repr(C)]
struct MinMaxInfo {
    reserved: Point,
    max_size: Point,
    max_position: Point,
    min_track_size: Point,
    max_track_size: Point,
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
    fn GetFocus() -> Hwnd;
    fn GetParent(window: Hwnd) -> Hwnd;
    fn GetDlgCtrlID(window: Hwnd) -> i32;
    fn DestroyWindow(window: Hwnd) -> i32;
    fn IsWindow(window: Hwnd) -> i32;
    fn SetForegroundWindow(window: Hwnd) -> i32;
    fn EnableWindow(window: Hwnd, enabled: i32) -> i32;
    fn SetWindowPos(
        window: Hwnd,
        insert_after: Hwnd,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    fn SetTimer(window: Hwnd, id: usize, interval: u32, callback: *const c_void) -> usize;
    fn KillTimer(window: Hwnd, id: usize) -> i32;
    fn GetDC(window: Hwnd) -> Hdc;
    fn ReleaseDC(window: Hwnd, dc: Hdc) -> i32;
    fn FillRect(dc: Hdc, rectangle: *const Rect, brush: Hbrush) -> i32;
    fn SetScrollInfo(window: Hwnd, bar: i32, info: *const ScrollInfo, redraw: i32) -> i32;
    fn GetScrollInfo(window: Hwnd, bar: i32, info: *mut ScrollInfo) -> i32;
    fn MessageBoxW(window: Hwnd, text: *const u16, caption: *const u16, kind: u32) -> i32;
    fn DrawTextW(dc: Hdc, text: *const u16, length: i32, rectangle: *mut Rect, format: u32) -> i32;
    fn TrackMouseEvent(event: *mut TrackMouseEventData) -> i32;
    fn SetProcessDpiAwarenessContext(context: Handle) -> i32;
    fn GetDpiForSystem() -> u32;
    fn GetDpiForWindow(window: Hwnd) -> u32;
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
    fn AddFontMemResourceEx(
        font: *const c_void,
        size: u32,
        reserved: *mut c_void,
        count: *mut u32,
    ) -> Handle;
    fn RemoveFontMemResourceEx(font: Handle) -> i32;
    fn SelectObject(dc: Hdc, object: Hgdiobj) -> Hgdiobj;
    fn DeleteObject(object: Hgdiobj) -> i32;
    fn SetTextColor(dc: Hdc, color: u32) -> u32;
    fn SetBkColor(dc: Hdc, color: u32) -> u32;
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
    fn CombineRgn(destination: Hrgn, source1: Hrgn, source2: Hrgn, mode: i32) -> i32;
    fn CreateDIBSection(
        dc: Hdc,
        info: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: Handle,
        offset: u32,
    ) -> Hbitmap;
    fn CreateCompatibleDC(dc: Hdc) -> Hdc;
    fn CreateCompatibleBitmap(dc: Hdc, width: i32, height: i32) -> Hbitmap;
    fn DeleteDC(dc: Hdc) -> i32;
    fn BitBlt(
        destination: Hdc,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        source: Hdc,
        source_x: i32,
        source_y: i32,
        raster_operation: u32,
    ) -> i32;
}

#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(
        window: Hwnd,
        subclass_proc: Option<SubclassProc>,
        subclass_id: usize,
        reference_data: usize,
    ) -> i32;
    fn DefSubclassProc(window: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
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
        // Per-monitor V2 keeps the custom chrome crisp as windows move between displays.
        // A failure is harmless when a host process has already selected a DPI mode.
        SetProcessDpiAwarenessContext(-4_isize as Handle);
        let initial_dpi = GetDpiForSystem().max(DEFAULT_DPI);
        let instance = GetModuleHandleW(null());
        if instance.is_null() {
            return Err(last_error("locate application module"));
        }
        register_class(instance, MAIN_CLASS, main_window_proc, COLOR_WINDOW)?;
        register_class(instance, TASK_CLASS, task_window_proc, COLOR_WINDOW)?;

        let options = LaunchOptions::parse(process_started)?;
        let benchmark_is_hidden = options.benchmark.is_some();
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
            scale_dip(DEFAULT_WINDOW_WIDTH_DIP, initial_dpi),
            scale_dip(DEFAULT_WINDOW_HEIGHT_DIP, initial_dpi),
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
        let mut screenshot = None;
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
                "--screenshot" => {
                    screenshot = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or_else(|| "--screenshot requires a path".to_string())?,
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
                html_parse_time: Duration::ZERO,
                resource_processing_time: Duration::ZERO,
                script_time: Duration::ZERO,
                style_refresh_time: Duration::ZERO,
                layout_time: Duration::ZERO,
                status: 0,
                bytes: 0,
                final_url: String::new(),
                error: None,
                script_executed: 0,
                script_mutations: 0,
                script_errors: Vec::new(),
                script_console: Vec::new(),
                script_diagnostics: Vec::new(),
                script_runtime_stopped: false,
                finish_scheduled: false,
                screenshot,
            })
        } else {
            if screenshot.is_some() {
                return Err("--screenshot requires --benchmark".to_string());
            }
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
    html_parse_time: Duration,
    resource_processing_time: Duration,
    script_time: Duration,
    style_refresh_time: Duration,
    layout_time: Duration,
    status: u32,
    bytes: u64,
    final_url: String,
    error: Option<String>,
    script_executed: usize,
    script_mutations: usize,
    script_errors: Vec<String>,
    script_console: Vec<String>,
    script_diagnostics: Vec<String>,
    script_runtime_stopped: bool,
    finish_scheduled: bool,
    screenshot: Option<PathBuf>,
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
    ui: Hfont,
    ui_semibold: Hfont,
    ui_small: Hfont,
    body: Hfont,
    small: Hfont,
    heading1: Hfont,
    heading2: Hfont,
    heading3: Hfont,
    mono: Hfont,
}

impl Fonts {
    unsafe fn create(dpi: u32) -> Result<Self, String> {
        let fonts = Self {
            ui: create_font(scaled_font_height(-16, dpi), 400, false, "Segoe UI"),
            ui_semibold: create_font(scaled_font_height(-16, dpi), 600, false, "Segoe UI"),
            ui_small: create_font(scaled_font_height(-14, dpi), 400, false, "Segoe UI"),
            body: create_font(scaled_font_height(-19, dpi), 400, false, "Segoe UI"),
            small: create_font(scaled_font_height(-16, dpi), 400, false, "Segoe UI"),
            heading1: create_font(scaled_font_height(-34, dpi), 600, false, "Segoe UI"),
            heading2: create_font(scaled_font_height(-28, dpi), 600, false, "Segoe UI"),
            heading3: create_font(scaled_font_height(-23, dpi), 600, false, "Segoe UI"),
            mono: create_font(scaled_font_height(-18, dpi), 400, false, "Cascadia Mono"),
        };
        if [
            fonts.ui,
            fonts.ui_semibold,
            fonts.ui_small,
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
                self.ui,
                self.ui_semibold,
                self.ui_small,
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
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ChromeLayout {
    address_frame: Rect,
    status: Rect,
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
    unsafe fn get_or_create(&mut self, spec: &FontSpec, dpi: u32) -> Hfont {
        let key = font_key(spec, dpi);
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

    unsafe fn clear(&mut self) {
        for font in self.fonts.drain().map(|(_, font)| font) {
            if !font.is_null() {
                DeleteObject(font);
            }
        }
    }
}

fn font_key(spec: &FontSpec, dpi: u32) -> FontKey {
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
        size: (spec.size * dpi_scale(dpi)).round().clamp(1.0, 768.0) as i32,
        weight: spec.weight.clamp(100, 900),
        italic: spec.italic,
        underline: spec.underline,
    }
}

impl Drop for DynamicFonts {
    fn drop(&mut self) {
        unsafe { self.clear() }
    }
}

#[derive(Default)]
struct WebFontResources {
    handles: Vec<Handle>,
}

impl WebFontResources {
    unsafe fn register(&mut self, fonts: &[WebFont]) {
        self.clear();
        for font in fonts {
            let Ok(size) = u32::try_from(font.sfnt.len()) else {
                continue;
            };
            let mut count = 0_u32;
            let handle =
                AddFontMemResourceEx(font.sfnt.as_ptr().cast(), size, null_mut(), &mut count);
            if !handle.is_null() && count > 0 {
                self.handles.push(handle);
            }
        }
    }

    unsafe fn clear(&mut self) {
        for handle in self.handles.drain(..) {
            if !handle.is_null() {
                RemoveFontMemResourceEx(handle);
            }
        }
    }
}

impl Drop for WebFontResources {
    fn drop(&mut self) {
        unsafe { self.clear() }
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

    unsafe fn get_or_create_tinted(
        &mut self,
        key: &str,
        image: &DecodedImage,
        tint: [u8; 4],
        dc: Hdc,
    ) -> Hbitmap {
        let cache_key = format!(
            "{key}#tint:{:02x}{:02x}{:02x}{:02x}",
            tint[0], tint[1], tint[2], tint[3]
        );
        if let Some(bitmap) = self.bitmaps.get(&cache_key) {
            return *bitmap;
        }

        let mut tinted = Vec::with_capacity(image.bgra.len());
        for pixel in image.bgra.chunks_exact(4) {
            let alpha = u16::from(pixel[3]) * u16::from(tint[3]) / 255;
            tinted.push((u16::from(tint[2]) * alpha / 255) as u8);
            tinted.push((u16::from(tint[1]) * alpha / 255) as u8);
            tinted.push((u16::from(tint[0]) * alpha / 255) as u8);
            tinted.push(alpha as u8);
        }

        let info = bitmap_info(image);
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        if !bitmap.is_null() && !pixels.is_null() {
            std::ptr::copy_nonoverlapping(tinted.as_ptr(), pixels.cast(), tinted.len());
            self.bitmaps.insert(cache_key, bitmap);
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
    dpi: u32,
}

impl TextMeasurer for GdiTextMeasurer<'_> {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        unsafe {
            let handle = self.fonts.get_or_create(font, self.dpi);
            SelectObject(self.dc, handle);
            let size = measure_text(self.dc, text);
            let scale = dpi_scale(self.dpi);
            (size.cx as f32 / scale, size.cy as f32 / scale)
        }
    }
}

struct PageControlWindow {
    window: Hwnd,
    spec: better_web_browser::engine::ControlSpec,
    brush: Hbrush,
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
    web_fonts: WebFontResources,
    image_bitmaps: ImageBitmaps,
    content_brush: Hbrush,
    omnibox_brush: Hbrush,
    dpi: u32,
    chrome: ChromeLayout,
    status_text: String,
    page: Page,
    document: Option<Document>,
    reader_html: String,
    reader_url: String,
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
    http_client: Arc<winhttp::HttpClient>,
    task_window: Hwnd,
}

impl BrowserState {
    fn new(
        instance: Hinstance,
        metrics: Arc<BrowserMetrics>,
        options: LaunchOptions,
    ) -> Result<Self, String> {
        let home = parse_html(HOME_HTML, HOME_URL);
        let page = Page::parse(HOME_HTML, HOME_URL);
        let http_client = Arc::new(winhttp::HttpClient::new()?);
        Ok(Self {
            instance,
            window: null_mut(),
            controls: Controls::default(),
            fonts: None,
            dynamic_fonts: DynamicFonts::default(),
            web_fonts: WebFontResources::default(),
            image_bitmaps: ImageBitmaps::default(),
            content_brush: unsafe { CreateSolidBrush(rgb(250, 250, 248)) },
            omnibox_brush: unsafe { CreateSolidBrush(CHROME_THEME.field) },
            dpi: DEFAULT_DPI,
            chrome: ChromeLayout::default(),
            status_text: "Ready".to_string(),
            page,
            document: Some(home),
            reader_html: HOME_HTML.to_string(),
            reader_url: HOME_URL.to_string(),
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
            http_client,
            task_window: null_mut(),
        })
    }

    unsafe fn create_controls(&mut self) -> Result<(), String> {
        self.dpi = window_dpi(self.window);
        self.fonts = Some(Fonts::create(self.dpi)?);
        let button_style = BS_OWNERDRAW | WS_TABSTOP;
        self.controls.back = self.create_control("BUTTON", "Back", button_style, ID_BACK);
        self.controls.forward = self.create_control("BUTTON", "Forward", button_style, ID_FORWARD);
        self.controls.reload = self.create_control("BUTTON", "Reload", button_style, ID_RELOAD);
        self.controls.address =
            self.create_control("EDIT", "", WS_TABSTOP | ES_AUTOHSCROLL, ID_ADDRESS);
        self.controls.go = self.create_control("BUTTON", "Go", button_style, ID_GO);
        self.controls.task_manager =
            self.create_control("BUTTON", "Task manager", button_style, ID_TASK_MANAGER);
        self.controls.reader = self.create_control("BUTTON", "Reader", button_style, ID_READER);

        let all = [
            self.controls.back,
            self.controls.forward,
            self.controls.reload,
            self.controls.address,
            self.controls.go,
            self.controls.task_manager,
            self.controls.reader,
        ];
        if all.iter().any(|window| window.is_null()) {
            return Err(last_error("create browser controls"));
        }
        let font = self.fonts.as_ref().unwrap().ui;
        for control in all {
            SendMessageW(control, WM_SETFONT, font as usize, 1);
            SetWindowSubclass(
                control,
                Some(chrome_control_proc),
                1,
                GetDlgCtrlID(control).max(0) as usize,
            );
        }
        let cue = wide("Search or enter an address");
        SendMessageW(
            self.controls.address,
            EM_SETCUEBANNER,
            1,
            cue.as_ptr() as isize,
        );
        SendMessageW(self.controls.address, EM_SETMARGINS, 0x0003, 0);
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

    fn scale(&self, dip: i32) -> i32 {
        scale_dip(dip, self.dpi)
    }

    fn page_scale(&self) -> f32 {
        dpi_scale(self.dpi)
    }

    fn toolbar_height(&self) -> i32 {
        self.scale(TOOLBAR_HEIGHT_DIP)
    }

    fn status_height(&self) -> i32 {
        self.scale(STATUS_HEIGHT_DIP)
    }

    unsafe fn apply_dpi(&mut self, dpi: u32) -> Result<(), String> {
        let dpi = dpi.max(DEFAULT_DPI);
        if dpi == self.dpi {
            return Ok(());
        }
        let fonts = Fonts::create(dpi)?;
        self.dpi = dpi;
        self.fonts = Some(fonts);
        self.dynamic_fonts.clear();

        let interface_font = self.fonts.as_ref().unwrap().ui;
        for control in [
            self.controls.back,
            self.controls.forward,
            self.controls.reload,
            self.controls.address,
            self.controls.go,
            self.controls.task_manager,
            self.controls.reader,
        ] {
            if !control.is_null() {
                SendMessageW(control, WM_SETFONT, interface_font as usize, 1);
            }
        }
        let page_font = self.fonts.as_ref().unwrap().body;
        for control in &self.page_controls {
            SendMessageW(control.window, WM_SETFONT, page_font as usize, 1);
        }
        Ok(())
    }

    unsafe fn resize_controls(&mut self) {
        let mut rectangle: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut rectangle);
        let width = rectangle.right.max(1);
        let height = rectangle.bottom.max(1);
        let compact = width < self.scale(760);
        let very_compact = width < self.scale(520);
        let margin = self.scale(if very_compact { 7 } else { 12 });
        let gap = self.scale(if very_compact { 2 } else { 4 });
        let group_gap = self.scale(if very_compact { 5 } else { 9 });
        let control_height = self.scale(40);
        let nav_width = self.scale(if very_compact { 34 } else { 40 });
        let top = ((self.toolbar_height() - control_height) / 2).max(0);

        let mut left = margin;
        for control in [
            self.controls.back,
            self.controls.forward,
            self.controls.reload,
        ] {
            MoveWindow(control, left, top, nav_width, control_height, 1);
            left += nav_width + gap;
        }

        let task_width = self.scale(if compact { 42 } else { 116 });
        let reader_width = self.scale(if compact { 42 } else { 78 });
        let go_width = self.scale(if very_compact { 40 } else { 48 });
        let task_left = (width - margin - task_width).max(left);
        let reader_left = (task_left - gap - reader_width).max(left);
        let go_left = (reader_left - gap - go_width).max(left);

        MoveWindow(self.controls.go, go_left, top, go_width, control_height, 1);
        MoveWindow(
            self.controls.reader,
            reader_left,
            top,
            reader_width,
            control_height,
            1,
        );
        MoveWindow(
            self.controls.task_manager,
            task_left,
            top,
            task_width,
            control_height,
            1,
        );

        let address_left = left + group_gap - gap;
        let address_right = (go_left - group_gap).max(address_left + 1);
        self.chrome.address_frame = Rect {
            left: address_left,
            top,
            right: address_right,
            bottom: top + control_height,
        };
        let horizontal_inset = self.scale(13);
        let vertical_inset = self.scale(8);
        MoveWindow(
            self.controls.address,
            self.chrome.address_frame.left + horizontal_inset,
            self.chrome.address_frame.top + vertical_inset,
            (self.chrome.address_frame.right
                - self.chrome.address_frame.left
                - horizontal_inset * 2)
                .max(1),
            (control_height - vertical_inset * 2).max(1),
            1,
        );

        self.chrome.status = Rect {
            left: 0,
            top: (height - self.status_height()).max(self.toolbar_height()),
            right: width,
            bottom: height,
        };
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
        let http_client = Arc::clone(&self.http_client);
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let requested_viewport_width = client.right.max(1) as f32 / self.page_scale();
        let navigation_thread = std::thread::Builder::new()
            .name("breeze-navigation".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let _request = metrics.begin_request();
                let started = Instant::now();
                let result = (|| -> Result<LoadedPage, String> {
                    let client = http_client;
                    let mut response = client.get(&url)?;
                    let mut network_time = started.elapsed();
                    let mut bytes = response.body.len() as u64;
                    let mut resource_budget = 32_u64 * 1024 * 1024;
                    let mut visited = HashSet::from([response.final_url.clone()]);
                    let mut navigation_count = 0;
                    let mut html_parse_time = Duration::ZERO;
                    let mut resource_processing_time = Duration::ZERO;
                    let mut script_time = Duration::ZERO;
                    let mut style_refresh_time = Duration::ZERO;

                    let (
                        rendered_page,
                        html,
                        final_url,
                        status,
                        script_outcome,
                        deferred_resources,
                    ) = loop {
                        let final_url = response.final_url.clone();
                        let status = response.status;
                        let html =
                            winhttp::decode_text(&response.body, response.content_type.as_deref());
                        let html_parse_started = Instant::now();
                        let mut rendered_page = Page::parse_scripted(&html, &final_url);
                        html_parse_time += html_parse_started.elapsed();

                        if navigation_count < 5
                            && let Some(refresh_url) = rendered_page.immediate_refresh_url()
                            && visited.insert(refresh_url.clone())
                        {
                            let refresh_started = Instant::now();
                            let refresh_response = client.get(&refresh_url);
                            network_time += refresh_started.elapsed();
                            if let Ok(next_response) = refresh_response {
                                bytes += next_response.body.len() as u64;
                                visited.insert(next_response.final_url.clone());
                                response = next_response;
                                navigation_count += 1;
                                continue;
                            }
                        }

                        let mut loaded_resources = HashSet::new();
                        load_page_resources(
                            &client,
                            &mut rendered_page,
                            &mut loaded_resources,
                            &mut resource_budget,
                            &mut bytes,
                            &mut network_time,
                            &mut resource_processing_time,
                        );
                        let script_started = Instant::now();
                        let script_network_before = network_time;
                        let mut script_outcome = {
                            let mut dynamic_script_loader = |url: &str| -> Result<String, String> {
                                let request_started = Instant::now();
                                let response = client.get(url);
                                network_time += request_started.elapsed();
                                let response = response?;
                                if !response.is_success() {
                                    return Err(format!(
                                        "server returned HTTP {}",
                                        response.status
                                    ));
                                }
                                let size = response.body.len() as u64;
                                if size > resource_budget {
                                    return Err("page resource budget was exhausted".into());
                                }
                                let processing_started = Instant::now();
                                let code = winhttp::decode_text(
                                    &response.body,
                                    response.content_type.as_deref(),
                                );
                                resource_processing_time += processing_started.elapsed();
                                bytes += size;
                                resource_budget -= size;
                                Ok(code)
                            };
                            rendered_page
                                .execute_first_paint_scripts_with_loader(&mut dynamic_script_loader)
                        };
                        let dynamic_script_network =
                            network_time.saturating_sub(script_network_before);
                        script_time += script_started
                            .elapsed()
                            .saturating_sub(dynamic_script_network);
                        for cookie in &script_outcome.cookie_updates {
                            if let Err(error) = client.set_cookie(&final_url, cookie) {
                                script_outcome
                                    .errors
                                    .push(format!("document.cookie: {error}"));
                            }
                        }
                        if script_outcome.runtime_stopped {
                            let fallback_parse_started = Instant::now();
                            rendered_page = Page::parse(&html, &final_url);
                            html_parse_time += fallback_parse_started.elapsed();
                        } else {
                            let style_refresh_started = Instant::now();
                            rendered_page.refresh_resources(requested_viewport_width);
                            style_refresh_time += style_refresh_started.elapsed();
                            load_page_resources(
                                &client,
                                &mut rendered_page,
                                &mut loaded_resources,
                                &mut resource_budget,
                                &mut bytes,
                                &mut network_time,
                                &mut resource_processing_time,
                            );
                        }

                        if navigation_count < 5
                            && let Some(navigation_url) = script_outcome.navigation_url.clone()
                            && navigation_url != final_url
                            && visited.insert(navigation_url.clone())
                        {
                            let navigation_started = Instant::now();
                            let navigation_response = client.get(&navigation_url);
                            network_time += navigation_started.elapsed();
                            match navigation_response {
                                Ok(next_response) => {
                                    bytes += next_response.body.len() as u64;
                                    visited.insert(next_response.final_url.clone());
                                    response = next_response;
                                    navigation_count += 1;
                                    continue;
                                }
                                Err(error) => script_outcome.errors.push(format!(
                                    "{navigation_url}: script-requested navigation failed: {error}"
                                )),
                            }
                        }

                        let deferred_resources = rendered_page
                            .resources
                            .iter()
                            .filter(|resource| {
                                !loaded_resources.contains(*resource)
                                    && matches!(resource, PageResource::Font { .. })
                            })
                            .cloned()
                            .collect();
                        break (
                            rendered_page,
                            html,
                            final_url,
                            status,
                            script_outcome,
                            deferred_resources,
                        );
                    };
                    let parse_time = started.elapsed().saturating_sub(network_time);
                    metrics.record_success(bytes, parse_time.as_micros() as u64);
                    Ok(LoadedPage {
                        page: rendered_page,
                        html,
                        final_url,
                        status,
                        bytes,
                        network_time,
                        parse_time,
                        html_parse_time,
                        resource_processing_time,
                        script_time,
                        style_refresh_time,
                        script_outcome,
                        deferred_resources,
                    })
                })();
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
        if let Err(error) = navigation_thread {
            self.loading = false;
            self.set_status(&format!("Could not start navigation: {error}"));
        }
    }

    unsafe fn finish_navigation(&mut self, message: LoadMessage) {
        if message.generation != self.generation {
            return;
        }
        self.loading = false;
        match message.result {
            Ok(mut page) => {
                let deferred_resources = std::mem::take(&mut page.deferred_resources);
                self.destroy_page_controls();
                self.image_bitmaps.clear();
                self.dynamic_fonts.clear();
                self.web_fonts.clear();
                self.page = page.page;
                self.web_fonts.register(&self.page.fonts);
                self.document = None;
                self.reader_html = page.html;
                self.reader_url = page.final_url.clone();
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
                let layout_started = Instant::now();
                self.rebuild_layout();
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.network_time = page.network_time;
                    benchmark.parse_time = page.parse_time;
                    benchmark.html_parse_time = page.html_parse_time;
                    benchmark.resource_processing_time = page.resource_processing_time;
                    benchmark.script_time = page.script_time;
                    benchmark.style_refresh_time = page.style_refresh_time;
                    benchmark.status = page.status;
                    benchmark.bytes = page.bytes;
                    benchmark.final_url = page.final_url.clone();
                    benchmark.script_executed = page.script_outcome.executed;
                    benchmark.script_mutations = page.script_outcome.mutation_count;
                    benchmark.script_errors = page.script_outcome.errors.clone();
                    benchmark.script_console = page.script_outcome.console.clone();
                    benchmark.script_diagnostics = page.script_outcome.diagnostics.clone();
                    benchmark.script_runtime_stopped = page.script_outcome.runtime_stopped;
                }
                let script_status =
                    if page.script_outcome.executed == 0 && page.script_outcome.errors.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "  •  JS {} / {} mutations / {} errors",
                            page.script_outcome.executed,
                            page.script_outcome.mutation_count,
                            page.script_outcome.errors.len()
                        )
                    };
                self.set_status(&format!(
                    "HTTP {}  •  {}  •  network {}  •  parse {}{}",
                    page.status,
                    format_bytes(page.bytes),
                    format_duration(page.network_time),
                    format_duration(page.parse_time),
                    script_status
                ));
                InvalidateRect(self.window, null(), 0);
                UpdateWindow(self.window);
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.layout_time = layout_started.elapsed();
                    benchmark.page_ready = benchmark.process_started.elapsed();
                }
                self.schedule_benchmark_finish();
                self.begin_deferred_resources(deferred_resources);
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

    unsafe fn begin_deferred_resources(&self, resources: Vec<PageResource>) {
        if resources.is_empty() {
            return;
        }
        let generation = self.generation;
        let window = self.window as isize;
        let http_client = Arc::clone(&self.http_client);
        std::thread::spawn(move || {
            let client = http_client;
            let loaded = std::thread::scope(|scope| {
                let client = &client;
                let requests = resources
                    .into_iter()
                    .map(|resource| {
                        scope.spawn(move || {
                            let response = client.get(page_resource_url(&resource));
                            (resource, response)
                        })
                    })
                    .collect::<Vec<_>>();
                requests
                    .into_iter()
                    .filter_map(|request| request.join().ok())
                    .filter_map(|(resource, response)| {
                        response
                            .ok()
                            .filter(winhttp::HttpResponse::is_success)
                            .map(|response| (resource, response.body))
                    })
                    .collect::<Vec<_>>()
            });
            let message = Box::new(DeferredResourcesMessage { generation, loaded });
            let pointer = Box::into_raw(message);
            if unsafe {
                PostMessageW(
                    window as Hwnd,
                    WM_APP_DEFERRED_RESOURCES,
                    0,
                    pointer as isize,
                )
            } == 0
            {
                unsafe { drop(Box::from_raw(pointer)) };
            }
        });
    }

    unsafe fn finish_deferred_resources(&mut self, message: DeferredResourcesMessage) {
        if message.generation != self.generation {
            return;
        }
        let mut changed = false;
        for (resource, body) in message.loaded {
            if let PageResource::Font {
                url,
                family,
                weight,
                italic,
            } = resource
            {
                changed |= self
                    .page
                    .add_font(url, family, weight, italic, &body)
                    .is_ok();
            }
        }
        if changed {
            self.web_fonts.clear();
            self.web_fonts.register(&self.page.fonts);
            self.dynamic_fonts.clear();
            self.rebuild_layout();
            InvalidateRect(self.window, null(), 0);
        }
    }

    unsafe fn finish_benchmark(&mut self) {
        let Some(benchmark) = self.benchmark.as_ref() else {
            return;
        };
        let screenshot = benchmark.screenshot.clone();
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let viewport_width = client.right.max(1) as f32 / self.page_scale();
        let viewport_height = self.viewport_height().max(1) as f32 / self.page_scale();
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
        let script_errors = format!(
            "[{}]",
            benchmark
                .script_errors
                .iter()
                .map(|error| json_string(error))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let script_console = format!(
            "[{}]",
            benchmark
                .script_console
                .iter()
                .map(|message| json_string(message))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let script_diagnostics = format!(
            "[{}]",
            benchmark
                .script_diagnostics
                .iter()
                .map(|message| json_string(message))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let json = format!(
            concat!(
                "{{\n",
                "  \"browser\": {},\n",
                "  \"requested_url\": {},\n",
                "  \"final_url\": {},\n",
                "  \"error\": {},\n",
                "  \"http_status\": {},\n",
                "  \"viewport_width_css_px\": {:.3},\n",
                "  \"viewport_height_css_px\": {:.3},\n",
                "  \"window_ready_ms\": {:.3},\n",
                "  \"page_ready_ms\": {:.3},\n",
                "  \"navigation_ms\": {:.3},\n",
                "  \"network_ms\": {:.3},\n",
                "  \"parse_ms\": {:.3},\n",
                "  \"html_parse_ms\": {:.3},\n",
                "  \"resource_processing_ms\": {:.3},\n",
                "  \"javascript_ms\": {:.3},\n",
                "  \"style_refresh_ms\": {:.3},\n",
                "  \"layout_and_paint_ms\": {:.3},\n",
                "  \"settle_ms\": {},\n",
                "  \"working_set_bytes\": {},\n",
                "  \"private_bytes\": {},\n",
                "  \"peak_working_set_bytes\": {},\n",
                "  \"cpu_time_ms\": {:.3},\n",
                "  \"average_cpu_percent\": {:.3},\n",
                "  \"process_count\": 1,\n",
                "  \"downloaded_bytes\": {},\n",
                "  \"javascript_scripts_executed\": {},\n",
                "  \"javascript_dom_mutations\": {},\n",
                "  \"javascript_errors\": {},\n",
                "  \"javascript_console\": {},\n",
                "  \"javascript_diagnostics\": {},\n",
                "  \"javascript_runtime_stopped\": {},\n",
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
            viewport_width,
            viewport_height,
            benchmark.window_ready.as_secs_f64() * 1_000.0,
            benchmark.page_ready.as_secs_f64() * 1_000.0,
            navigation_ms,
            benchmark.network_time.as_secs_f64() * 1_000.0,
            benchmark.parse_time.as_secs_f64() * 1_000.0,
            benchmark.html_parse_time.as_secs_f64() * 1_000.0,
            benchmark.resource_processing_time.as_secs_f64() * 1_000.0,
            benchmark.script_time.as_secs_f64() * 1_000.0,
            benchmark.style_refresh_time.as_secs_f64() * 1_000.0,
            benchmark.layout_time.as_secs_f64() * 1_000.0,
            benchmark.settle.as_millis(),
            memory.working_set,
            memory.private_usage,
            memory.peak_working_set,
            cpu_seconds * 1_000.0,
            average_cpu,
            metrics.bytes_downloaded,
            benchmark.script_executed,
            benchmark.script_mutations,
            script_errors,
            script_console,
            script_diagnostics,
            benchmark.script_runtime_stopped,
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
        if let Some(path) = screenshot
            && let Err(error) = self.capture_screenshot(&path)
        {
            self.set_status(&format!("Failed to capture benchmark: {error}"));
        }
        DestroyWindow(self.window);
    }

    unsafe fn capture_screenshot(&mut self, path: &std::path::Path) -> Result<(), String> {
        let mut client: Rect = std::mem::zeroed();
        if GetClientRect(self.window, &mut client) == 0 {
            return Err(last_error("measure benchmark capture"));
        }
        let width = client.right.max(1);
        let height = client.bottom.max(1);
        let byte_len = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "benchmark capture is too large".to_string())?;

        let window_dc = GetDC(self.window);
        if window_dc.is_null() {
            return Err(last_error("open benchmark capture surface"));
        }
        let memory_dc = CreateCompatibleDC(window_dc);
        if memory_dc.is_null() {
            ReleaseDC(self.window, window_dc);
            return Err(last_error("create benchmark capture surface"));
        }
        let info = BitmapInfo {
            header: BitmapInfoHeader {
                size: size_of::<BitmapInfoHeader>() as u32,
                width,
                height: -height,
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: byte_len.min(u32::MAX as usize) as u32,
                x_pixels_per_meter: 0,
                y_pixels_per_meter: 0,
                colors_used: 0,
                colors_important: 0,
            },
            colors: [0],
        };
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(window_dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        if bitmap.is_null() || pixels.is_null() {
            DeleteDC(memory_dc);
            ReleaseDC(self.window, window_dc);
            return Err(last_error("allocate benchmark capture bitmap"));
        }

        let previous = SelectObject(memory_dc, bitmap);
        self.paint_surface(memory_dc, &client);
        if let Some(fonts) = self.fonts.as_ref() {
            SelectObject(memory_dc, fonts.ui);
            SetTextColor(memory_dc, CHROME_THEME.text);
            SetBkMode(memory_dc, TRANSPARENT);
            let mut address_rect = self
                .chrome
                .address_frame
                .inset(self.scale(16), self.scale(1));
            let address = window_text(self.controls.address);
            draw_text_in_rect(
                memory_dc,
                &address,
                &mut address_rect,
                DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }
        let bgra = std::slice::from_raw_parts(pixels.cast::<u8>(), byte_len);
        let mut rgba = Vec::with_capacity(byte_len);
        for pixel in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }

        if !previous.is_null() {
            SelectObject(memory_dc, previous);
        }
        DeleteObject(bitmap);
        DeleteDC(memory_dc);
        ReleaseDC(self.window, window_dc);

        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create screenshot directory: {error}"))?;
        }
        image::save_buffer(
            path,
            &rgba,
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )
        .map_err(|error| format!("write screenshot: {error}"))
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

    unsafe fn set_status(&mut self, status: &str) {
        self.status_text.clear();
        self.status_text.push_str(status);
        if !self.window.is_null() {
            InvalidateRect(self.window, &self.chrome.status, 0);
            let toolbar = Rect {
                left: 0,
                top: 0,
                right: self.chrome.status.right,
                bottom: self.toolbar_height(),
            };
            InvalidateRect(self.window, &toolbar, 0);
        }
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
                let scale = self.page_scale();
                let viewport_width = client.right.max(1) as f32 / scale;
                let viewport_height = self.viewport_height() as f32 / scale;
                let mut measurer = GdiTextMeasurer {
                    dc,
                    fonts: &mut self.dynamic_fonts,
                    dpi: self.dpi,
                };
                self.page_layout =
                    layout_page(&self.page, viewport_width, viewport_height, &mut measurer);
                self.content_height = (self.page_layout.content_height * scale).ceil() as i32;
                self.metrics
                    .set_retained_draw_items(self.page_layout.items.len());
            }
            Surface::Reader => {
                let Some(fonts) = self.fonts.as_ref() else {
                    ReleaseDC(self.window, dc);
                    return;
                };
                let content_margin = self.scale(CONTENT_MARGIN_DIP);
                let available = (client.right - content_margin * 2).max(self.scale(220));
                let reading_width = available.min(self.scale(MAX_READING_WIDTH_DIP));
                let left = ((client.right - reading_width) / 2).max(content_margin);
                let Some(document) = self.document.as_ref() else {
                    ReleaseDC(self.window, dc);
                    return;
                };
                let (items, height) = layout_document(dc, fonts, document, left, reading_width);
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
        if self.surface == Surface::Page && self.document.is_none() {
            self.document = Some(parse_html(&self.reader_html, &self.reader_url));
        }
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
        InvalidateRect(self.controls.reader, null(), 0);
        self.scroll_y = 0;
        self.rebuild_layout();
        InvalidateRect(self.window, null(), 0);
    }

    unsafe fn destroy_page_controls(&mut self) {
        for control in self.page_controls.drain(..) {
            if !control.window.is_null() && IsWindow(control.window) != 0 {
                DestroyWindow(control.window);
            }
            if !control.brush.is_null() {
                DeleteObject(control.brush);
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
        let previous_selections = self
            .page_controls
            .iter()
            .filter(|control| control.spec.kind == ControlKind::Select)
            .filter_map(|control| {
                let selected = SendMessageW(control.window, CB_GETCURSEL, 0, 0);
                (selected >= 0).then_some((control.spec.node_id, selected as usize))
            })
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
                    ("BUTTON", BS_OWNERDRAW | WS_TABSTOP, spec.label.clone())
                }
                ControlKind::Select => (
                    "COMBOBOX",
                    CBS_DROPDOWNLIST | WS_TABSTOP | WS_VSCROLL,
                    String::new(),
                ),
                ControlKind::Password => (
                    "EDIT",
                    WS_TABSTOP | ES_AUTOHSCROLL | ES_PASSWORD,
                    previous_values
                        .get(&spec.node_id)
                        .cloned()
                        .unwrap_or_else(|| spec.value.clone()),
                ),
                ControlKind::TextArea => (
                    "EDIT",
                    WS_TABSTOP | ES_MULTILINE | ES_AUTOVSCROLL,
                    previous_values
                        .get(&spec.node_id)
                        .cloned()
                        .unwrap_or_else(|| spec.value.clone()),
                ),
                _ => (
                    "EDIT",
                    WS_TABSTOP | ES_AUTOHSCROLL,
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
            let font = self.dynamic_fonts.get_or_create(&spec.font, self.dpi);
            SendMessageW(window, WM_SETFONT, font as usize, 1);
            if spec.kind == ControlKind::Select {
                for option in &spec.options {
                    let label = wide(&option.label);
                    SendMessageW(window, CB_ADDSTRING, 0, label.as_ptr() as isize);
                }
                let selected = previous_selections
                    .get(&spec.node_id)
                    .copied()
                    .unwrap_or(spec.selected_index)
                    .min(spec.options.len().saturating_sub(1));
                SendMessageW(window, CB_SETCURSEL, selected, 0);
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
            let brush = CreateSolidBrush(spec.background_color.to_colorref());
            self.page_controls.push(PageControlWindow {
                window,
                spec,
                brush,
            });
        }
        self.sync_page_control_positions();
    }

    unsafe fn sync_page_control_positions(&self) {
        let viewport_height = self.viewport_height();
        let toolbar_height = self.toolbar_height();
        let scale = self.page_scale();
        for control in &self.page_controls {
            let rect = control.spec.rect;
            let full_screen_y = toolbar_height + (rect.y * scale).round() as i32 - self.scroll_y;
            let full_height = (rect.height * scale).ceil().max(1.0) as i32;
            let visible = full_screen_y + full_height >= toolbar_height
                && full_screen_y <= toolbar_height + viewport_height;
            if visible {
                let is_button = matches!(
                    control.spec.kind,
                    ControlKind::Submit | ControlKind::Button | ControlKind::Reset
                );
                let [border_top, border_right, border_bottom, border_left] =
                    control.spec.border_width;
                let [padding_top, padding_right, padding_bottom, padding_left] =
                    control.spec.padding;
                let (left_inset, top_inset, right_inset, bottom_inset) = if is_button {
                    (0.0, 0.0, 0.0, 0.0)
                } else {
                    (
                        border_left + padding_left,
                        border_top + padding_top,
                        border_right + padding_right,
                        border_bottom + padding_bottom,
                    )
                };
                let x = ((rect.x + left_inset) * scale).round() as i32;
                let y = full_screen_y + (top_inset * scale).round() as i32;
                let width =
                    ((rect.width - left_inset - right_inset).max(1.0) * scale).ceil() as i32;
                let height =
                    ((rect.height - top_inset - bottom_inset).max(1.0) * scale).ceil() as i32;
                let native_height = if control.spec.kind == ControlKind::Select {
                    height + self.scale(220)
                } else {
                    height
                };
                MoveWindow(control.window, x, y, width, native_height, 1);
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
                if page_control.spec.form_id != Some(form_id) {
                    continue;
                }
                if matches!(
                    page_control.spec.kind,
                    ControlKind::Text
                        | ControlKind::TextArea
                        | ControlKind::Password
                        | ControlKind::Search
                ) {
                    set_window_text(page_control.window, &page_control.spec.value);
                } else if page_control.spec.kind == ControlKind::Select {
                    SendMessageW(
                        page_control.window,
                        CB_SETCURSEL,
                        page_control.spec.selected_index,
                        0,
                    );
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
                ControlKind::Select => {
                    let selected = SendMessageW(page_control.window, CB_GETCURSEL, 0, 0);
                    let value = (selected >= 0)
                        .then_some(selected as usize)
                        .and_then(|index| page_control.spec.options.get(index))
                        .map(|option| option.value.clone())
                        .unwrap_or_else(|| page_control.spec.value.clone());
                    fields.push((page_control.spec.name.clone(), value));
                }
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
        (client.bottom - self.toolbar_height() - self.status_height()).max(1)
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
        let toolbar_height = self.toolbar_height();
        if y < toolbar_height || y > toolbar_height + self.viewport_height() {
            return;
        }
        let url = match self.surface {
            Surface::Page => {
                let scale = self.page_scale();
                let document_x = x as f32 / scale;
                let document_y = (y - toolbar_height + self.scroll_y) as f32 / scale;
                self.page_layout.items.iter().find_map(|item| match item {
                    DisplayItem::Text {
                        rect,
                        link: Some(link),
                        ..
                    } if document_x >= rect.x
                        && document_x <= rect.right()
                        && document_y >= rect.y
                        && document_y <= rect.bottom() =>
                    {
                        Some(link.clone())
                    }
                    _ => None,
                })
            }
            Surface::Reader => {
                let document_y = y - toolbar_height + self.scroll_y;
                self.draw_items
                    .iter()
                    .find(|item| {
                        item.link.is_some()
                            && x >= item.x
                            && x <= item.x + item.width
                            && document_y >= item.y
                            && document_y <= item.y + item.height
                    })
                    .and_then(|item| item.link.clone())
            }
        };
        if let Some(url) = url {
            self.begin_navigation(url, HistoryMode::Push);
        }
    }

    unsafe fn paint(&mut self) {
        let mut paint: PaintStruct = std::mem::zeroed();
        let window_dc = BeginPaint(self.window, &mut paint);
        if window_dc.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let width = client.right.max(1);
        let height = client.bottom.max(1);
        let memory_dc = CreateCompatibleDC(window_dc);
        let bitmap = if memory_dc.is_null() {
            null_mut()
        } else {
            CreateCompatibleBitmap(window_dc, width, height)
        };

        if !memory_dc.is_null() && !bitmap.is_null() {
            let previous = SelectObject(memory_dc, bitmap);
            self.paint_surface(memory_dc, &client);
            BitBlt(window_dc, 0, 0, width, height, memory_dc, 0, 0, SRCCOPY);
            if !previous.is_null() {
                SelectObject(memory_dc, previous);
            }
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
        } else {
            if !memory_dc.is_null() {
                DeleteDC(memory_dc);
            }
            self.paint_surface(window_dc, &client);
        }
        EndPaint(self.window, &paint);
    }

    unsafe fn paint_surface(&mut self, dc: Hdc, client: &Rect) {
        let toolbar_height = self.toolbar_height();
        let scale = self.page_scale();
        let content = Rect {
            left: 0,
            top: toolbar_height,
            right: client.right,
            bottom: (client.bottom - self.status_height()).max(toolbar_height),
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
                            let rectangle =
                                screen_rect(*rect, self.scroll_y, toolbar_height, scale);
                            if intersects(&rectangle, &content) {
                                fill_color_shape(
                                    dc,
                                    &rectangle,
                                    color.to_colorref(),
                                    *radius * scale,
                                );
                            }
                        }
                        DisplayItem::BorderRect {
                            rect,
                            widths,
                            color,
                            radius,
                        } => {
                            let rectangle =
                                screen_rect(*rect, self.scroll_y, toolbar_height, scale);
                            if intersects(&rectangle, &content) {
                                paint_border(
                                    dc,
                                    &rectangle,
                                    widths.map(|width| width * scale),
                                    color.to_colorref(),
                                    *radius * scale,
                                );
                            }
                        }
                        DisplayItem::Text {
                            rect,
                            text,
                            font,
                            color,
                            ..
                        } => {
                            let screen_y =
                                toolbar_height + (rect.y * scale).round() as i32 - self.scroll_y;
                            if screen_y + ((rect.height * scale).ceil() as i32) < content.top
                                || screen_y > content.bottom
                            {
                                continue;
                            }
                            let font_handle = self.dynamic_fonts.get_or_create(font, self.dpi);
                            SelectObject(dc, font_handle);
                            SetTextColor(dc, color.to_colorref());
                            let text = wide_without_null(text);
                            TextOutW(
                                dc,
                                (rect.x * scale).round() as i32,
                                screen_y,
                                text.as_ptr(),
                                text.len() as i32,
                            );
                        }
                        DisplayItem::Image {
                            rect,
                            url,
                            alt,
                            tint,
                        } => {
                            let screen_y =
                                toolbar_height + (rect.y * scale).round() as i32 - self.scroll_y;
                            if screen_y + ((rect.height * scale).ceil() as i32) < content.top
                                || screen_y > content.bottom
                            {
                                continue;
                            }
                            if let Some(image) = self.page.images.get(url) {
                                let bitmap = if let Some(color) = tint {
                                    self.image_bitmaps.get_or_create_tinted(
                                        url,
                                        image,
                                        [color.red, color.green, color.blue, color.alpha],
                                        dc,
                                    )
                                } else {
                                    self.image_bitmaps.get_or_create(url, image, dc)
                                };
                                if !bitmap.is_null() {
                                    paint_alpha_image(dc, bitmap, image, *rect, screen_y, scale);
                                }
                            } else if !alt.is_empty()
                                && let Some(fonts) = self.fonts.as_ref()
                            {
                                SelectObject(dc, fonts.body);
                                SetTextColor(dc, rgb(70, 70, 70));
                                let alt = wide_without_null(alt);
                                TextOutW(
                                    dc,
                                    (rect.x * scale).round() as i32,
                                    screen_y,
                                    alt.as_ptr(),
                                    alt.len() as i32,
                                );
                            }
                        }
                        DisplayItem::BackgroundImage {
                            clip_rect,
                            tile_rect,
                            url,
                            repeat_x,
                            repeat_y,
                        } => {
                            let clip =
                                screen_rect(*clip_rect, self.scroll_y, toolbar_height, scale);
                            if !intersects(&clip, &content)
                                || tile_rect.width <= 0.0
                                || tile_rect.height <= 0.0
                            {
                                continue;
                            }
                            if let Some(image) = self.page.images.get(url) {
                                let bitmap = self.image_bitmaps.get_or_create(url, image, dc);
                                if !bitmap.is_null() {
                                    paint_background_image(
                                        dc,
                                        bitmap,
                                        image,
                                        *clip_rect,
                                        *tile_rect,
                                        *repeat_x,
                                        *repeat_y,
                                        self.scroll_y,
                                        toolbar_height,
                                        scale,
                                    );
                                }
                            }
                        }
                        DisplayItem::Control(spec) => {
                            if self.benchmark.is_some() {
                                let mut rectangle =
                                    screen_rect(spec.rect, self.scroll_y, toolbar_height, scale);
                                if !intersects(&rectangle, &content) {
                                    continue;
                                }
                                let is_button = matches!(
                                    spec.kind,
                                    ControlKind::Submit | ControlKind::Button | ControlKind::Reset
                                );
                                if !is_button {
                                    let [border_top, border_right, border_bottom, border_left] =
                                        spec.border_width
                                            .map(|width| (width * scale).ceil() as i32);
                                    let [padding_top, padding_right, padding_bottom, padding_left] =
                                        spec.padding.map(|width| (width * scale).ceil() as i32);
                                    rectangle.left += border_left + padding_left;
                                    rectangle.top += border_top + padding_top;
                                    rectangle.right -= border_right + padding_right;
                                    rectangle.bottom -= border_bottom + padding_bottom;
                                }
                                let font = self.dynamic_fonts.get_or_create(&spec.font, self.dpi);
                                SelectObject(dc, font);
                                SetTextColor(
                                    dc,
                                    if spec.text_color.alpha == 0 {
                                        CHROME_THEME.text
                                    } else {
                                        spec.text_color.to_colorref()
                                    },
                                );
                                let value = self
                                    .page_controls
                                    .iter()
                                    .find(|control| control.spec.node_id == spec.node_id)
                                    .map(|control| window_text(control.window))
                                    .unwrap_or_else(|| spec.value.clone());
                                let text = if spec.kind == ControlKind::Password {
                                    "•".repeat(value.chars().count())
                                } else if value.is_empty() {
                                    if spec.kind == ControlKind::Select || is_button {
                                        spec.label.clone()
                                    } else {
                                        spec.placeholder.clone()
                                    }
                                } else {
                                    value
                                };
                                draw_text_in_rect(
                                    dc,
                                    &text,
                                    &mut rectangle,
                                    DT_VCENTER
                                        | DT_SINGLELINE
                                        | DT_END_ELLIPSIS
                                        | DT_NOPREFIX
                                        | if is_button { DT_CENTER } else { 0 },
                                );
                            }
                        }
                    }
                }
            }
            Surface::Reader => {
                if let Some(fonts) = self.fonts.as_ref() {
                    for item in &self.draw_items {
                        let screen_y = toolbar_height + item.y - self.scroll_y;
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
        self.paint_chrome(dc, client);
    }

    unsafe fn paint_chrome(&self, dc: Hdc, client: &Rect) {
        let toolbar = Rect {
            left: 0,
            top: 0,
            right: client.right,
            bottom: self.toolbar_height(),
        };
        fill_color_rect(dc, &toolbar, CHROME_THEME.toolbar);
        let hairline = self.scale(1).max(1);
        fill_color_rect(
            dc,
            &Rect {
                left: 0,
                top: toolbar.bottom - hairline,
                right: toolbar.right,
                bottom: toolbar.bottom,
            },
            CHROME_THEME.border,
        );

        let address_focused = GetFocus() == self.controls.address;
        paint_rounded_panel(
            dc,
            &self.chrome.address_frame,
            CHROME_THEME.field,
            if address_focused {
                CHROME_THEME.focus
            } else {
                CHROME_THEME.border
            },
            self.scale(10) as f32,
            self.scale(if address_focused { 2 } else { 1 }).max(1),
        );

        if self.loading {
            let progress_width = ((client.right as f32) * 0.36).round() as i32;
            fill_color_rect(
                dc,
                &Rect {
                    left: 0,
                    top: toolbar.bottom - self.scale(3).max(2),
                    right: progress_width,
                    bottom: toolbar.bottom,
                },
                CHROME_THEME.accent,
            );
        }

        fill_color_rect(dc, &self.chrome.status, CHROME_THEME.status);
        fill_color_rect(
            dc,
            &Rect {
                left: self.chrome.status.left,
                top: self.chrome.status.top,
                right: self.chrome.status.right,
                bottom: self.chrome.status.top + hairline,
            },
            CHROME_THEME.border,
        );
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, fonts.ui_small);
        SetTextColor(dc, CHROME_THEME.muted_text);
        let dot_size = self.scale(7);
        let dot_left = self.scale(14);
        let dot_top =
            self.chrome.status.top + ((self.chrome.status.height() - dot_size) / 2).max(0);
        fill_color_shape(
            dc,
            &Rect {
                left: dot_left,
                top: dot_top,
                right: dot_left + dot_size,
                bottom: dot_top + dot_size,
            },
            if self.loading {
                CHROME_THEME.accent
            } else {
                CHROME_THEME.success
            },
            dot_size as f32 / 2.0,
        );
        let mut text_rect = Rect {
            left: dot_left + dot_size + self.scale(9),
            top: self.chrome.status.top,
            right: (self.chrome.status.right - self.scale(12)).max(1),
            bottom: self.chrome.status.bottom,
        };
        draw_text_in_rect(
            dc,
            &self.status_text,
            &mut text_rect,
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }

    unsafe fn paint_chrome_button(&self, item: &DrawItemStruct) {
        let id = item.control_id as usize;
        let hovered = GetWindowLongPtrW(item.item_window, GWLP_USERDATA) != 0;
        let pressed = item.item_state & ODS_SELECTED != 0;
        let disabled = item.item_state & ODS_DISABLED != 0;
        let focused = item.item_state & ODS_FOCUS != 0;
        let active = id == ID_READER && self.surface == Surface::Reader;
        let primary = id == ID_GO;

        fill_color_rect(item.dc, &item.item_rect, CHROME_THEME.toolbar);
        let fill = if primary {
            if pressed {
                CHROME_THEME.accent_pressed
            } else if hovered {
                CHROME_THEME.accent_hover
            } else {
                CHROME_THEME.accent
            }
        } else if pressed {
            CHROME_THEME.pressed
        } else if active {
            CHROME_THEME.accent_soft
        } else if hovered {
            CHROME_THEME.hover
        } else {
            CHROME_THEME.toolbar
        };
        let mut button = item.item_rect.inset(self.scale(1), self.scale(1));
        if focused {
            paint_rounded_panel(
                item.dc,
                &button,
                fill,
                CHROME_THEME.focus,
                self.scale(9) as f32,
                self.scale(2).max(1),
            );
        } else {
            fill_color_shape(item.dc, &button, fill, self.scale(9) as f32);
        }

        let compact = button.width() < self.scale(70);
        let label = match id {
            ID_BACK => "←",
            ID_FORWARD => "→",
            ID_RELOAD => "↻",
            ID_READER if compact => "Aa",
            ID_READER => "Reader",
            ID_TASK_MANAGER if compact => "⋯",
            ID_TASK_MANAGER => "Task manager",
            ID_GO => "Go",
            _ => "",
        };
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };
        let icon = matches!(id, ID_BACK | ID_FORWARD | ID_RELOAD);
        SelectObject(
            item.dc,
            if icon {
                fonts.heading3
            } else {
                fonts.ui_semibold
            },
        );
        SetBkMode(item.dc, TRANSPARENT);
        SetTextColor(
            item.dc,
            if disabled {
                CHROME_THEME.disabled_text
            } else if primary {
                CHROME_THEME.field
            } else if active {
                CHROME_THEME.accent
            } else {
                CHROME_THEME.text
            },
        );
        if pressed {
            let offset = self.scale(1);
            button.top += offset;
            button.bottom += offset;
        }
        draw_text_in_rect(
            item.dc,
            label,
            &mut button,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }

    unsafe fn paint_page_button(&mut self, item: &DrawItemStruct, index: usize) {
        let Some(control) = self.page_controls.get(index) else {
            return;
        };
        let spec = control.spec.clone();
        let scale = self.page_scale();
        let radius = spec.border_radius * scale;
        fill_color_shape(
            item.dc,
            &item.item_rect,
            spec.background_color.to_colorref(),
            radius,
        );
        if spec.border_color.alpha > 0 && spec.border_width.iter().any(|width| *width > 0.0) {
            paint_border(
                item.dc,
                &item.item_rect,
                spec.border_width.map(|width| width * scale),
                spec.border_color.to_colorref(),
                radius,
            );
        }
        if item.item_state & ODS_FOCUS != 0 {
            let focus_rect = item.item_rect.inset(self.scale(1), self.scale(1));
            paint_border(
                item.dc,
                &focus_rect,
                [1.0; 4],
                CHROME_THEME.focus,
                (radius - 1.0).max(0.0),
            );
        }

        if spec.label.is_empty()
            && let Some(icon_url) = spec.icon_url.as_deref()
            && let Some(image) = self.page.images.get(icon_url)
        {
            let bitmap = self.image_bitmaps.get_or_create_tinted(
                icon_url,
                image,
                [
                    spec.text_color.red,
                    spec.text_color.green,
                    spec.text_color.blue,
                    spec.text_color.alpha,
                ],
                item.dc,
            );
            if !bitmap.is_null() {
                let horizontal_inset = (spec.padding[1]
                    + spec.padding[3]
                    + spec.border_width[1]
                    + spec.border_width[3])
                    * scale;
                let vertical_inset = (spec.padding[0]
                    + spec.padding[2]
                    + spec.border_width[0]
                    + spec.border_width[2])
                    * scale;
                let available_width = (item.item_rect.width() as f32 - horizontal_inset).max(1.0);
                let available_height = (item.item_rect.height() as f32 - vertical_inset).max(1.0);
                let requested_width = (spec.icon_width * scale).max(1.0);
                let requested_height = (spec.icon_height * scale).max(1.0);
                let fit = (available_width / requested_width)
                    .min(available_height / requested_height)
                    .min(1.0);
                let width = (requested_width * fit).round().max(1.0) as i32;
                let height = (requested_height * fit).round().max(1.0) as i32;
                let pressed_offset = if item.item_state & ODS_SELECTED != 0 {
                    self.scale(1)
                } else {
                    0
                };
                let icon_rect = Rect {
                    left: item.item_rect.left + (item.item_rect.width() - width) / 2,
                    top: item.item_rect.top
                        + (item.item_rect.height() - height) / 2
                        + pressed_offset,
                    right: item.item_rect.left + (item.item_rect.width() + width) / 2,
                    bottom: item.item_rect.top
                        + (item.item_rect.height() + height) / 2
                        + pressed_offset,
                };
                paint_alpha_bitmap(item.dc, bitmap, image, &icon_rect);
            }
            return;
        }

        let font = self.dynamic_fonts.get_or_create(&spec.font, self.dpi);
        SelectObject(item.dc, font);
        SetBkMode(item.dc, TRANSPARENT);
        SetTextColor(item.dc, spec.text_color.to_colorref());
        let mut text_rect = item.item_rect;
        if item.item_state & ODS_SELECTED != 0 {
            text_rect.top += self.scale(1);
            text_rect.bottom += self.scale(1);
        }
        draw_text_in_rect(
            item.dc,
            &spec.label,
            &mut text_rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
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
            self.scale(600),
            self.scale(560),
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
            if !self.omnibox_brush.is_null() {
                DeleteObject(self.omnibox_brush);
            }
        }
    }
}

enum HistoryMode {
    Push,
    Existing,
}

fn load_page_resources(
    client: &winhttp::HttpClient,
    page: &mut Page,
    loaded: &mut HashSet<PageResource>,
    resource_budget: &mut u64,
    bytes: &mut u64,
    network_time: &mut Duration,
    resource_processing_time: &mut Duration,
) {
    const MAX_PARALLEL_FETCHES: usize = 24;
    let resources = page
        .resources
        .iter()
        .filter(|resource| {
            !loaded.contains(*resource) && page.resource_blocks_first_paint(resource)
        })
        .cloned()
        .collect::<Vec<_>>();

    for batch in resources.chunks(MAX_PARALLEL_FETCHES) {
        if *resource_budget == 0 {
            break;
        }
        for resource in batch {
            loaded.insert(resource.clone());
        }

        let batch_started = Instant::now();
        let responses = std::thread::scope(|scope| {
            let requests = batch
                .iter()
                .map(|resource| scope.spawn(move || client.get(page_resource_url(resource))))
                .collect::<Vec<_>>();
            requests
                .into_iter()
                .map(|request| {
                    request.join().unwrap_or_else(|_| {
                        Err("resource request worker terminated unexpectedly".into())
                    })
                })
                .collect::<Vec<_>>()
        });
        *network_time += batch_started.elapsed();

        let processing_started = Instant::now();
        for (resource, response) in batch.iter().cloned().zip(responses) {
            let Ok(response) = response else {
                continue;
            };
            if !response.is_success() {
                continue;
            }
            let size = response.body.len() as u64;
            if size > *resource_budget {
                continue;
            }

            let retained = match resource {
                PageResource::Stylesheet { url } => {
                    page.add_stylesheet_from(
                        &url,
                        winhttp::decode_text(&response.body, response.content_type.as_deref()),
                    );
                    true
                }
                PageResource::Image { url } => page.add_image(url, &response.body).is_ok(),
                PageResource::Script { url } => {
                    page.add_script(
                        &url,
                        winhttp::decode_text(&response.body, response.content_type.as_deref()),
                    );
                    true
                }
                PageResource::Font {
                    url,
                    family,
                    weight,
                    italic,
                } => page
                    .add_font(url, family, weight, italic, &response.body)
                    .is_ok(),
            };
            if retained {
                *bytes += size;
                *resource_budget -= size;
                if *resource_budget == 0 {
                    break;
                }
            }
        }
        *resource_processing_time += processing_started.elapsed();
        if *resource_budget == 0 {
            break;
        }
    }
}

fn page_resource_url(resource: &PageResource) -> &str {
    match resource {
        PageResource::Stylesheet { url }
        | PageResource::Image { url }
        | PageResource::Script { url }
        | PageResource::Font { url, .. } => url,
    }
}

struct LoadedPage {
    page: Page,
    html: String,
    final_url: String,
    status: u32,
    bytes: u64,
    network_time: Duration,
    parse_time: Duration,
    html_parse_time: Duration,
    resource_processing_time: Duration,
    script_time: Duration,
    style_refresh_time: Duration,
    script_outcome: ScriptOutcome,
    deferred_resources: Vec<PageResource>,
}

struct DeferredResourcesMessage {
    generation: u64,
    loaded: Vec<(PageResource, Vec<u8>)>,
}

struct LoadMessage {
    generation: u64,
    result: Result<LoadedPage, String>,
}

unsafe extern "system" fn chrome_control_proc(
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
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam as *mut MinMaxInfo);
            info.min_track_size = Point {
                x: state.scale(500),
                y: state.scale(360),
            };
            0
        }
        WM_SIZE => {
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
            state.finish_navigation(*message);
            0
        }
        WM_APP_DEFERRED_RESOURCES => {
            let message = Box::from_raw(lparam as *mut DeferredResourcesMessage);
            state.finish_deferred_resources(*message);
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

    unsafe fn paint(&self) {
        let mut paint: PaintStruct = std::mem::zeroed();
        let window_dc = BeginPaint(self.window, &mut paint);
        if window_dc.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let width = client.right.max(1);
        let height = client.bottom.max(1);
        let memory_dc = CreateCompatibleDC(window_dc);
        let bitmap = if memory_dc.is_null() {
            null_mut()
        } else {
            CreateCompatibleBitmap(window_dc, width, height)
        };
        if !memory_dc.is_null() && !bitmap.is_null() {
            let previous = SelectObject(memory_dc, bitmap);
            self.paint_surface(memory_dc, &client);
            BitBlt(window_dc, 0, 0, width, height, memory_dc, 0, 0, SRCCOPY);
            if !previous.is_null() {
                SelectObject(memory_dc, previous);
            }
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
        } else {
            if !memory_dc.is_null() {
                DeleteDC(memory_dc);
            }
            self.paint_surface(window_dc, &client);
        }
        EndPaint(self.window, &paint);
    }

    unsafe fn paint_surface(&self, dc: Hdc, client: &Rect) {
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };
        fill_color_rect(dc, client, CHROME_THEME.task_background);
        SetBkMode(dc, TRANSPARENT);

        let header_height = self.scale(68);
        let margin = self.scale(20);
        let gap = self.scale(12);
        fill_color_rect(
            dc,
            &Rect {
                left: 0,
                top: 0,
                right: client.right,
                bottom: header_height,
            },
            CHROME_THEME.card,
        );
        fill_color_rect(
            dc,
            &Rect {
                left: 0,
                top: header_height - self.scale(1).max(1),
                right: client.right,
                bottom: header_height,
            },
            CHROME_THEME.border,
        );
        paint_text(
            dc,
            fonts.title,
            CHROME_THEME.text,
            "Performance",
            Rect {
                left: margin,
                top: self.scale(10),
                right: client.right - margin,
                bottom: self.scale(39),
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        paint_text(
            dc,
            fonts.small,
            CHROME_THEME.muted_text,
            &format!("{PRODUCT_NAME} · one owned browser process"),
            Rect {
                left: margin,
                top: self.scale(38),
                right: client.right - margin - self.scale(86),
                bottom: self.scale(60),
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        let live = Rect {
            left: (client.right - margin - self.scale(72)).max(margin),
            top: self.scale(20),
            right: client.right - margin,
            bottom: self.scale(48),
        };
        paint_rounded_panel(
            dc,
            &live,
            rgb(232, 247, 239),
            rgb(198, 231, 214),
            self.scale(14) as f32,
            self.scale(1).max(1),
        );
        let dot = self.scale(7);
        fill_color_shape(
            dc,
            &Rect {
                left: live.left + self.scale(11),
                top: live.top + (live.height() - dot) / 2,
                right: live.left + self.scale(11) + dot,
                bottom: live.top + (live.height() + dot) / 2,
            },
            CHROME_THEME.success,
            dot as f32 / 2.0,
        );
        paint_text(
            dc,
            fonts.heading,
            rgb(27, 112, 73),
            "LIVE",
            Rect {
                left: live.left + self.scale(24),
                top: live.top,
                right: live.right - self.scale(8),
                bottom: live.bottom,
            },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );

        let hero = Rect {
            left: margin,
            top: header_height + self.scale(14),
            right: (client.right - margin).max(margin + 1),
            bottom: header_height + self.scale(114),
        };
        paint_rounded_panel(
            dc,
            &hero,
            CHROME_THEME.card,
            CHROME_THEME.border,
            self.scale(12) as f32,
            self.scale(1).max(1),
        );
        let split = hero.left + hero.width() * 58 / 100;
        paint_text(
            dc,
            fonts.heading,
            CHROME_THEME.muted_text,
            "CPU USAGE",
            Rect {
                left: hero.left + self.scale(16),
                top: hero.top + self.scale(10),
                right: split - self.scale(10),
                bottom: hero.top + self.scale(30),
            },
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        paint_text(
            dc,
            fonts.metric,
            CHROME_THEME.text,
            &self.view.cpu,
            Rect {
                left: hero.left + self.scale(16),
                top: hero.top + self.scale(29),
                right: split - self.scale(10),
                bottom: hero.top + self.scale(67),
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        paint_text(
            dc,
            fonts.small,
            CHROME_THEME.muted_text,
            &format!(
                "Normalized across {} logical processors",
                self.logical_processors
            ),
            Rect {
                left: hero.left + self.scale(16),
                top: hero.top + self.scale(65),
                right: split - self.scale(10),
                bottom: hero.top + self.scale(84),
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        let bar = Rect {
            left: hero.left + self.scale(16),
            top: hero.bottom - self.scale(13),
            right: split - self.scale(16),
            bottom: hero.bottom - self.scale(8),
        };
        fill_color_shape(dc, &bar, CHROME_THEME.hover, self.scale(3) as f32);
        let filled = Rect {
            right: bar.left
                + ((bar.width() as f64 * self.cpu_percent / 100.0).round() as i32)
                    .max(self.scale(3)),
            ..bar
        };
        fill_color_shape(dc, &filled, CHROME_THEME.accent, self.scale(3) as f32);
        fill_color_rect(
            dc,
            &Rect {
                left: split,
                top: hero.top + self.scale(16),
                right: split + self.scale(1).max(1),
                bottom: hero.bottom - self.scale(16),
            },
            CHROME_THEME.border,
        );
        self.paint_metric_cell(
            dc,
            fonts,
            Rect {
                left: split + self.scale(16),
                top: hero.top + self.scale(17),
                right: hero.right - self.scale(14),
                bottom: hero.bottom - self.scale(15),
            },
            "WORKING SET",
            &self.view.working_set,
        );

        let process = Rect {
            left: margin,
            top: hero.bottom + gap,
            right: (client.right - margin).max(margin + 1),
            bottom: hero.bottom + gap + self.scale(82),
        };
        paint_rounded_panel(
            dc,
            &process,
            CHROME_THEME.card,
            CHROME_THEME.border,
            self.scale(12) as f32,
            self.scale(1).max(1),
        );
        paint_text(
            dc,
            fonts.heading,
            CHROME_THEME.muted_text,
            "BROWSER PROCESS",
            Rect {
                left: process.left + self.scale(16),
                top: process.top + self.scale(8),
                right: process.right - self.scale(16),
                bottom: process.top + self.scale(27),
            },
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        let handles_and_uptime = format!("{} · {}", self.view.handles, self.view.uptime);
        let process_values = [
            ("PRIVATE MEMORY", self.view.private_memory.as_str()),
            ("PEAK WORKING SET", self.view.peak_working_set.as_str()),
            ("HANDLES · UPTIME", handles_and_uptime.as_str()),
        ];
        let third = process.width() / 3;
        for (index, (label, value)) in process_values.iter().enumerate() {
            let left = process.left + third * index as i32;
            self.paint_metric_cell(
                dc,
                fonts,
                Rect {
                    left: left + self.scale(16),
                    top: process.top + self.scale(29),
                    right: if index == 2 {
                        process.right - self.scale(12)
                    } else {
                        left + third - self.scale(8)
                    },
                    bottom: process.bottom - self.scale(8),
                },
                label,
                value,
            );
        }

        let engine = Rect {
            left: margin,
            top: process.bottom + gap,
            right: (client.right - margin).max(margin + 1),
            bottom: (client.bottom - self.scale(16)).max(process.bottom + gap + self.scale(90)),
        };
        paint_rounded_panel(
            dc,
            &engine,
            CHROME_THEME.card,
            CHROME_THEME.border,
            self.scale(12) as f32,
            self.scale(1).max(1),
        );
        paint_text(
            dc,
            fonts.heading,
            CHROME_THEME.muted_text,
            "OWNED DOCUMENT ENGINE",
            Rect {
                left: engine.left + self.scale(16),
                top: engine.top + self.scale(9),
                right: engine.right - self.scale(16),
                bottom: engine.top + self.scale(29),
            },
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        let engine_values = [
            ("ACTIVE REQUESTS", self.view.active_requests.as_str()),
            ("PAGES COMPLETED", self.view.pages_completed.as_str()),
            ("FAILED LOADS", self.view.failed_loads.as_str()),
            ("DOWNLOADED", self.view.downloaded.as_str()),
            ("LAST HTML PARSE", self.view.last_parse.as_str()),
            ("DRAW ITEMS", self.view.draw_items.as_str()),
        ];
        let columns = if client.width() >= self.scale(500) {
            3
        } else {
            2
        };
        let rows = engine_values.len().div_ceil(columns);
        let grid_top = engine.top + self.scale(31);
        let cell_width = engine.width() / columns as i32;
        let cell_height = (engine.bottom - grid_top).max(1) / rows as i32;
        for (index, (label, value)) in engine_values.iter().enumerate() {
            let column = index % columns;
            let row = index / columns;
            self.paint_metric_cell(
                dc,
                fonts,
                Rect {
                    left: engine.left + cell_width * column as i32 + self.scale(16),
                    top: grid_top + cell_height * row as i32,
                    right: if column + 1 == columns {
                        engine.right - self.scale(12)
                    } else {
                        engine.left + cell_width * (column + 1) as i32 - self.scale(8)
                    },
                    bottom: grid_top + cell_height * (row + 1) as i32,
                },
                label,
                value,
            );
        }
    }

    unsafe fn paint_metric_cell(
        &self,
        dc: Hdc,
        fonts: &TaskManagerFonts,
        rectangle: Rect,
        label: &str,
        value: &str,
    ) {
        let split = rectangle.top + (rectangle.height() * 43 / 100).max(self.scale(14));
        paint_text(
            dc,
            fonts.small,
            CHROME_THEME.muted_text,
            label,
            Rect {
                bottom: split,
                ..rectangle
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        paint_text(
            dc,
            fonts.value,
            CHROME_THEME.text,
            value,
            Rect {
                top: split - self.scale(1),
                ..rectangle
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
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

fn screen_rect(rect: RectF, scroll_y: i32, content_top: i32, scale: f32) -> Rect {
    Rect {
        left: (rect.x * scale).round() as i32,
        top: content_top + (rect.y * scale).round() as i32 - scroll_y,
        right: (rect.right() * scale).ceil() as i32,
        bottom: content_top + (rect.bottom() * scale).ceil() as i32 - scroll_y,
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
    scale: f32,
) {
    let destination_rect = Rect {
        left: (rect.x * scale).round() as i32,
        top: screen_y,
        right: ((rect.x + rect.width) * scale).round() as i32,
        bottom: screen_y + (rect.height * scale).round().max(1.0) as i32,
    };
    paint_alpha_bitmap(destination, bitmap, image, &destination_rect);
}

#[allow(clippy::too_many_arguments)]
unsafe fn paint_background_image(
    destination: Hdc,
    bitmap: Hbitmap,
    image: &DecodedImage,
    clip_rect: RectF,
    tile_rect: RectF,
    repeat_x: bool,
    repeat_y: bool,
    scroll_y: i32,
    content_top: i32,
    scale: f32,
) {
    if tile_rect.width <= 0.0 || tile_rect.height <= 0.0 {
        return;
    }
    let clip = screen_rect(clip_rect, scroll_y, content_top, scale);
    let saved = SaveDC(destination);
    IntersectClipRect(destination, clip.left, clip.top, clip.right, clip.bottom);

    let start_x = if repeat_x {
        tile_rect.x + ((clip_rect.x - tile_rect.x) / tile_rect.width).floor() * tile_rect.width
    } else {
        tile_rect.x
    };
    let start_y = if repeat_y {
        tile_rect.y + ((clip_rect.y - tile_rect.y) / tile_rect.height).floor() * tile_rect.height
    } else {
        tile_rect.y
    };
    let mut painted = 0_usize;
    let mut y = start_y;
    loop {
        let mut x = start_x;
        loop {
            let tile = RectF {
                x,
                y,
                width: tile_rect.width,
                height: tile_rect.height,
            };
            let destination_rect = screen_rect(tile, scroll_y, content_top, scale);
            paint_alpha_bitmap(destination, bitmap, image, &destination_rect);
            painted += 1;
            if !repeat_x || painted >= 4_096 {
                break;
            }
            x += tile_rect.width;
            if x >= clip_rect.right() {
                break;
            }
        }
        if !repeat_y || painted >= 4_096 {
            break;
        }
        y += tile_rect.height;
        if y >= clip_rect.bottom() {
            break;
        }
    }

    if saved != 0 {
        RestoreDC(destination, saved);
    }
}

unsafe fn paint_alpha_bitmap(
    destination: Hdc,
    bitmap: Hbitmap,
    image: &DecodedImage,
    destination_rect: &Rect,
) {
    let source = CreateCompatibleDC(destination);
    if source.is_null() {
        return;
    }
    let previous = SelectObject(source, bitmap);
    AlphaBlend(
        destination,
        destination_rect.left,
        destination_rect.top,
        destination_rect.width().max(1),
        destination_rect.height().max(1),
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

unsafe fn paint_rounded_panel(
    dc: Hdc,
    rectangle: &Rect,
    fill: u32,
    border: u32,
    radius: f32,
    border_width: i32,
) {
    if rectangle.width() <= 0 || rectangle.height() <= 0 {
        return;
    }
    let border_width = border_width.max(0);
    if border_width == 0 {
        fill_color_shape(dc, rectangle, fill, radius);
        return;
    }
    fill_color_shape(dc, rectangle, border, radius);
    let inner = rectangle.inset(border_width, border_width);
    if inner.width() > 0 && inner.height() > 0 {
        fill_color_shape(dc, &inner, fill, (radius - border_width as f32).max(0.0));
    }
}

unsafe fn draw_text_in_rect(dc: Hdc, text: &str, rectangle: &mut Rect, format: u32) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let text = wide_without_null(text);
    DrawTextW(dc, text.as_ptr(), text.len() as i32, rectangle, format)
}

unsafe fn paint_text(
    dc: Hdc,
    font: Hfont,
    color: u32,
    text: &str,
    mut rectangle: Rect,
    format: u32,
) {
    SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT);
    draw_text_in_rect(dc, text, &mut rectangle, format);
}

unsafe fn paint_border(dc: Hdc, rectangle: &Rect, widths: [f32; 4], color: u32, radius: f32) {
    let [top, right, bottom, left] = widths.map(|width| width.ceil().max(0.0) as i32);
    if radius > 0.0 {
        let brush = CreateSolidBrush(color);
        if brush.is_null() {
            return;
        }
        let diameter = (radius * 2.0).round().max(1.0) as i32;
        let outer = CreateRoundRectRgn(
            rectangle.left,
            rectangle.top,
            rectangle.right + 1,
            rectangle.bottom + 1,
            diameter,
            diameter,
        );
        let inner_rect = Rect {
            left: rectangle.left + left,
            top: rectangle.top + top,
            right: rectangle.right - right,
            bottom: rectangle.bottom - bottom,
        };
        if !outer.is_null() {
            if inner_rect.width() > 0 && inner_rect.height() > 0 {
                let border_width = top.max(right).max(bottom).max(left) as f32;
                let inner_radius = (radius - border_width).max(0.0);
                let inner_diameter = (inner_radius * 2.0).round().max(1.0) as i32;
                let inner = CreateRoundRectRgn(
                    inner_rect.left,
                    inner_rect.top,
                    inner_rect.right + 1,
                    inner_rect.bottom + 1,
                    inner_diameter,
                    inner_diameter,
                );
                if !inner.is_null() {
                    CombineRgn(outer, outer, inner, RGN_DIFF);
                    DeleteObject(inner);
                }
            }
            FillRgn(dc, outer, brush);
            DeleteObject(outer);
        }
        DeleteObject(brush);
        return;
    }
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

fn dpi_scale(dpi: u32) -> f32 {
    dpi.max(1) as f32 / DEFAULT_DPI as f32
}

fn scale_dip(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi.max(1) as i64 + (DEFAULT_DPI as i64 / 2)) / DEFAULT_DPI as i64)
        .clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

fn scaled_font_height(height: i32, dpi: u32) -> i32 {
    if height < 0 {
        -scale_dip(-height, dpi).max(1)
    } else {
        scale_dip(height, dpi).max(1)
    }
}

unsafe fn window_dpi(window: Hwnd) -> u32 {
    let dpi = GetDpiForWindow(window);
    if dpi == 0 { DEFAULT_DPI } else { dpi }
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
