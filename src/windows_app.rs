#![allow(unsafe_op_in_unsafe_fn)]

mod async_scripts;
mod benchmark;
mod document_activation;
mod document_navigation;
mod platform;
mod reader_layout;
mod resources;
mod runtime;
mod runtime_metrics;

use benchmark::{BenchmarkRun, LaunchOptions};
use document_activation::{LoadMessage, LoadedPage};
use platform::*;
use reader_layout::layout_document;
#[cfg(test)]
use reader_layout::words_with_spacing;
use resources::{DeferredResourcesMessage, load_page_resources};

use better_web_browser::branding::{BENCHMARK_ID, HOME_HTML, HOME_URL, PRODUCT_NAME};
use better_web_browser::document::{BlockKind, Document, Span, parse_html};
use better_web_browser::engine::{
    ControlKind, DecodedImage, DisplayItem, FontSpec, LayoutOutput, Page, PageResource, RectF,
    ScriptOutcome, ScriptRuntime, TextMeasurer, WebFont, layout_page_with_style_viewport,
};
use better_web_browser::metrics::BrowserMetrics;
use better_web_browser::navigation::{encode_www_form_component, normalize_user_input};
use better_web_browser::winhttp;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    calls: usize,
}

impl TextMeasurer for GdiTextMeasurer<'_> {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        unsafe {
            let handle = self.fonts.get_or_create(font, self.dpi);
            SelectObject(self.dc, handle);
            let size = measure_text(self.dc, text);
            self.calls += 1;
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
    script_runtime: Option<ScriptRuntime>,
    script_runtime_clock: Option<Instant>,
    loaded_page_resources: HashSet<PageResource>,
    page_resource_budget: u64,
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
    script_navigation: document_navigation::ScriptNavigationGuard,
    generation: u64,
    loading: bool,
    startup_url: Option<String>,
    open_task_manager_on_start: bool,
    benchmark: Option<BenchmarkRun>,
    metrics: Arc<BrowserMetrics>,
    http_client: Arc<winhttp::HttpClient>,
    task_window: Hwnd,
    last_layout_tree_time: Duration,
    last_layout_finalize_time: Duration,
    last_text_measure_count: usize,
    media_viewport_width: f32,
    outer_window_width: i32,
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
            script_runtime: None,
            script_runtime_clock: None,
            loaded_page_resources: HashSet::new(),
            page_resource_budget: PAGE_RESOURCE_BUDGET,
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
            script_navigation: document_navigation::ScriptNavigationGuard::default(),
            generation: 0,
            loading: false,
            startup_url: options.startup_url,
            open_task_manager_on_start: options.open_task_manager,
            benchmark: options.benchmark,
            metrics,
            http_client,
            task_window: null_mut(),
            last_layout_tree_time: Duration::ZERO,
            last_layout_finalize_time: Duration::ZERO,
            last_text_measure_count: 0,
            media_viewport_width: 0.0,
            outer_window_width: 0,
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

        Ok(())
    }

    unsafe fn complete_startup(&mut self) {
        self.reset_media_viewport_width();
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

    unsafe fn reset_media_viewport_width(&mut self) {
        let mut client: Rect = std::mem::zeroed();
        if GetClientRect(self.window, &mut client) != 0 {
            self.media_viewport_width = client.right.max(1) as f32 / self.page_scale();
        }
        let mut outer: Rect = std::mem::zeroed();
        if GetWindowRect(self.window, &mut outer) != 0 {
            self.outer_window_width = outer.width();
        }
    }

    unsafe fn track_media_viewport_resize(&mut self) {
        let mut outer: Rect = std::mem::zeroed();
        if GetWindowRect(self.window, &mut outer) == 0 {
            return;
        }
        let width = outer.width();
        if self.outer_window_width == 0 || self.media_viewport_width <= 0.0 {
            self.reset_media_viewport_width();
            return;
        }
        let physical_delta = width - self.outer_window_width;
        // A classic scrollbar can change the Win32 client width without changing the CSS
        // media viewport. Only an outer-window resize changes the media-query width.
        if physical_delta != 0 {
            self.media_viewport_width = resized_media_viewport_width(
                self.media_viewport_width,
                physical_delta,
                self.page_scale(),
            );
        }
        self.outer_window_width = width;
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
        self.cancel_script_runtime();
        if self.loading {
            self.generation = self.generation.wrapping_add(1);
        }
        match history_mode {
            HistoryMode::Push => {
                self.script_navigation.reset(&url);
                if self.history.get(self.history_index) != Some(&url) {
                    if !self.history.is_empty() {
                        self.history.truncate(self.history_index + 1);
                    }
                    self.history.push(url.clone());
                    self.history_index = self.history.len() - 1;
                }
            }
            HistoryMode::Existing => self.script_navigation.reset(&url),
            HistoryMode::Script => {}
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
                    let mut resource_budget = PAGE_RESOURCE_BUDGET;
                    let mut visited = HashSet::from([response.final_url.clone()]);
                    let mut navigation_count = 0;
                    let mut html_parse_time = Duration::ZERO;
                    let mut resource_processing_time = Duration::ZERO;
                    let (
                        rendered_page,
                        html,
                        final_url,
                        status,
                        loaded_resources,
                        remaining_resource_budget,
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
                        break (
                            rendered_page,
                            html,
                            final_url,
                            status,
                            loaded_resources,
                            resource_budget,
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
                        loaded_resources,
                        remaining_resource_budget,
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
        let layout_started = Instant::now();
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let dc = GetDC(self.window);
        if dc.is_null() {
            return;
        }
        self.last_text_measure_count = 0;
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
                    calls: 0,
                };
                let style_viewport_width = if self.media_viewport_width > 0.0 {
                    self.media_viewport_width
                } else {
                    viewport_width
                };
                self.page_layout = layout_page_with_style_viewport(
                    &self.page,
                    viewport_width,
                    viewport_height,
                    style_viewport_width,
                    &mut measurer,
                );
                self.last_text_measure_count = measurer.calls;
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
        self.last_layout_tree_time = layout_started.elapsed();
        self.clamp_scroll();
        self.update_scrollbar();
        self.recreate_page_controls();
        self.last_layout_finalize_time = layout_started
            .elapsed()
            .saturating_sub(self.last_layout_tree_time);
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
                DisplayItem::Control(spec) => Some((**spec).clone()),
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
            self.cancel_script_runtime();
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
    Script,
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
            state.track_media_viewport_resize();
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
        WM_APP_ASYNC_SCRIPT => {
            let message = Box::from_raw(lparam as *mut async_scripts::AsyncScriptMessage);
            state.finish_async_script(*message);
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

fn resized_media_viewport_width(current: f32, physical_delta: i32, scale: f32) -> f32 {
    (current + physical_delta as f32 / scale.max(f32::EPSILON)).max(1.0)
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

    #[test]
    fn tracks_media_viewport_across_physical_resizes() {
        assert_eq!(resized_media_viewport_width(1100.0, 125, 1.25), 1200.0);
        assert_eq!(resized_media_viewport_width(10.0, -100, 1.0), 1.0);
    }
}
