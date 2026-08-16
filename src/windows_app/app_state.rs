//! Browser-window ownership and application lifecycle state.

use super::browser_navigation::HistoryMode;
use super::renderer_lifecycle::{RendererTaskRegistry, SharedRendererRegistry};
use super::tab_state::{BrowserTab, ClosedTab};
use super::tabs::{RecentlyClosedTabs, TabCollection, TabId};
use super::*;
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;

pub(super) struct BrowserState {
    pub(super) instance: Hinstance,
    pub(super) window: Hwnd,
    pub(super) controls: Controls,
    pub(super) fonts: Option<Fonts>,
    pub(super) content_brush: Hbrush,
    pub(super) omnibox_brush: Hbrush,
    pub(super) dpi: u32,
    pub(super) chrome: ChromeLayout,
    pub(super) processing_background_tab: bool,
    pub(super) tabs: TabCollection<BrowserTab>,
    pub(super) recently_closed_tabs: RecentlyClosedTabs<ClosedTab>,
    pub(super) startup_url: Option<String>,
    pub(super) open_task_manager_on_start: bool,
    pub(super) benchmark: Option<BenchmarkRun>,
    pub(super) metrics: Arc<BrowserMetrics>,
    pub(super) http_client: Arc<winhttp::HttpClient>,
    pub(super) task_window: Hwnd,
    pub(super) renderer_registry: SharedRendererRegistry,
    pub(super) media_viewport_width: f32,
    pub(super) outer_window_width: i32,
}

impl BrowserState {
    pub(super) fn new(
        instance: Hinstance,
        metrics: Arc<BrowserMetrics>,
        options: LaunchOptions,
    ) -> Result<Self, String> {
        let http_client = Arc::new(winhttp::HttpClient::new()?);
        let tabs = TabCollection::new(BrowserTab::new(TabId::first()));
        Ok(Self {
            instance,
            window: null_mut(),
            controls: Controls::default(),
            fonts: None,
            content_brush: unsafe { CreateSolidBrush(rgb(250, 250, 248)) },
            omnibox_brush: unsafe { CreateSolidBrush(CHROME_THEME.field) },
            dpi: DEFAULT_DPI,
            chrome: ChromeLayout::default(),
            processing_background_tab: false,
            tabs,
            recently_closed_tabs: RecentlyClosedTabs::new(),
            startup_url: options.startup_url,
            open_task_manager_on_start: options.open_task_manager,
            benchmark: options.benchmark,
            metrics,
            http_client,
            task_window: null_mut(),
            renderer_registry: Arc::new(Mutex::new(RendererTaskRegistry::default())),
            media_viewport_width: 0.0,
            outer_window_width: 0,
        })
    }

    pub(super) unsafe fn complete_startup(&mut self) {
        self.reset_media_viewport_width();
        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.window_ready = benchmark.process_started.elapsed();
        }
        self.start_renderer();
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
        if !self.processing_background_tab && !self.window.is_null() {
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
            Arc::clone(&self.renderer_registry),
        ) {
            Ok(window) => self.task_window = window,
            Err(error) => self.set_status(&error),
        }
    }
}

impl Deref for BrowserState {
    type Target = BrowserTab;

    fn deref(&self) -> &Self::Target {
        self.tabs.active()
    }
}

impl DerefMut for BrowserState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.tabs.active_mut()
    }
}

impl Drop for BrowserState {
    fn drop(&mut self) {
        unsafe {
            self.document_fetch.abort();
            self.cancel_script_runtime();
            KillTimer(self.window, ID_RENDERER_MONITOR_TIMER);
            self.renderer_session.take();
            if !self.content_brush.is_null() {
                DeleteObject(self.content_brush);
            }
            if !self.omnibox_brush.is_null() {
                DeleteObject(self.omnibox_brush);
            }
        }
    }
}
