#![allow(unsafe_op_in_unsafe_fn)]

mod async_scripts;
mod benchmark;
mod chrome_controls;
mod chrome_paint;
mod document_activation;
mod document_navigation;
mod page_controls;
mod paint_primitives;
mod painting;
mod platform;
mod process_metrics;
mod reader_layout;
mod rendering_resources;
mod resources;
mod runtime;
mod runtime_metrics;
mod task_manager;

use benchmark::{BenchmarkRun, LaunchOptions};
use chrome_controls::{ChromeLayout, Controls};
use document_activation::{LoadMessage, LoadedPage};
use page_controls::PageControlWindow;
use platform::*;
use process_metrics::{process_cpu_ticks, process_memory};
use reader_layout::layout_document;
#[cfg(test)]
use reader_layout::words_with_spacing;
use rendering_resources::{
    DynamicFonts, FontKind, Fonts, GdiTextMeasurer, ImageBitmaps, WebFontResources,
};
use resources::{DeferredResourcesMessage, load_page_resources};

use better_web_browser::branding::{BENCHMARK_ID, HOME_HTML, HOME_URL, PRODUCT_NAME};
use better_web_browser::document::{BlockKind, Document, Span, parse_html};
use better_web_browser::engine::{
    ControlKind, DecodedImage, DisplayItem, FontSpec, LayoutOutput, Page, PageResource,
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
        register_class(
            instance,
            TASK_CLASS,
            task_manager::window_proc,
            COLOR_WINDOW,
        )?;

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

    unsafe fn open_task_manager(&mut self) {
        match task_manager::open(
            self.task_window,
            self.window,
            self.instance,
            self.dpi,
            Arc::clone(&self.metrics),
        ) {
            Ok(window) => self.task_window = window,
            Err(error) => self.set_status(&error),
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
