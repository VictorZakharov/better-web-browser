//! Browser-window ownership and application lifecycle state.

use super::browser_navigation::HistoryMode;
use super::page_controls::PageControlWindow;
use super::paint_index::PaintIndex;
use super::*;

pub(super) struct BrowserState {
    pub(super) instance: Hinstance,
    pub(super) window: Hwnd,
    pub(super) controls: Controls,
    pub(super) fonts: Option<Fonts>,
    pub(super) dynamic_fonts: DynamicFonts,
    pub(super) web_fonts: WebFontResources,
    pub(super) image_bitmaps: ImageBitmaps,
    pub(super) content_brush: Hbrush,
    pub(super) omnibox_brush: Hbrush,
    pub(super) dpi: u32,
    pub(super) chrome: ChromeLayout,
    pub(super) status_text: String,
    pub(super) page: Page,
    pub(super) script_runtime: Option<ScriptRuntime>,
    pub(super) script_runtime_clock: Option<Instant>,
    pub(super) loaded_page_resources: HashSet<PageResource>,
    pub(super) page_resource_budget: u64,
    pub(super) document: Option<Document>,
    pub(super) reader_html: String,
    pub(super) reader_url: String,
    pub(super) draw_items: Vec<DrawItem>,
    pub(super) page_layout: LayoutOutput,
    pub(super) paint_index: PaintIndex,
    pub(super) page_controls: Vec<PageControlWindow>,
    pub(super) surface: Surface,
    pub(super) content_height: i32,
    pub(super) scroll_y: i32,
    pub(super) history: Vec<String>,
    pub(super) history_index: usize,
    pub(super) script_navigation: document_navigation::ScriptNavigationGuard,
    pub(super) generation: u64,
    pub(super) loading: bool,
    pub(super) startup_url: Option<String>,
    pub(super) open_task_manager_on_start: bool,
    pub(super) benchmark: Option<BenchmarkRun>,
    pub(super) metrics: Arc<BrowserMetrics>,
    pub(super) http_client: Arc<winhttp::HttpClient>,
    pub(super) task_window: Hwnd,
    pub(super) last_layout_tree_time: Duration,
    pub(super) last_layout_finalize_time: Duration,
    pub(super) last_text_measure_count: usize,
    pub(super) media_viewport_width: f32,
    pub(super) outer_window_width: i32,
}

impl BrowserState {
    pub(super) fn new(
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
            paint_index: PaintIndex::default(),
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

    pub(super) unsafe fn complete_startup(&mut self) {
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

    pub(super) fn scale(&self, dip: i32) -> i32 {
        scale_dip(dip, self.dpi)
    }

    pub(super) fn page_scale(&self) -> f32 {
        dpi_scale(self.dpi)
    }

    pub(super) unsafe fn reset_media_viewport_width(&mut self) {
        let mut client: Rect = std::mem::zeroed();
        if GetClientRect(self.window, &mut client) != 0 {
            self.media_viewport_width = client.right.max(1) as f32 / self.page_scale();
        }
        let mut outer: Rect = std::mem::zeroed();
        if GetWindowRect(self.window, &mut outer) != 0 {
            self.outer_window_width = outer.width();
        }
    }

    pub(super) unsafe fn track_media_viewport_resize(&mut self) {
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

    pub(super) fn toolbar_height(&self) -> i32 {
        self.scale(TOOLBAR_HEIGHT_DIP)
    }

    pub(super) fn status_height(&self) -> i32 {
        self.scale(STATUS_HEIGHT_DIP)
    }

    pub(super) unsafe fn set_status(&mut self, status: &str) {
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

    pub(super) unsafe fn open_task_manager(&mut self) {
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
