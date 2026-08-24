//! Omnibox navigation, history traversal, and network-load dispatch.

use super::tabs::TabId;
use super::*;
use better_web_browser::fetch::{FetchController, FetchRequest, FetchSignal};

pub(super) enum HistoryMode {
    Push,
    Existing,
    Script,
}

impl BrowserState {
    pub(super) unsafe fn navigate_from_address(&mut self) {
        let input = window_text(self.controls.address);
        self.navigate_from_input(&input, HistoryMode::Push);
    }

    pub(super) unsafe fn navigate_from_input(&mut self, input: &str, history_mode: HistoryMode) {
        match normalize_user_input(input) {
            Ok(url) => self.begin_navigation(url, history_mode),
            Err(error) => self.set_status(&error.to_string()),
        }
    }

    pub(super) unsafe fn begin_navigation(&mut self, url: String, history_mode: HistoryMode) {
        self.begin_navigation_for_tab(self.tabs.active_id(), url, history_mode);
    }

    pub(super) unsafe fn begin_navigation_for_tab(
        &mut self,
        id: TabId,
        url: String,
        history_mode: HistoryMode,
    ) {
        let is_active = self.tabs.active_id() == id && !self.processing_background_tab;
        if is_active {
            self.cancel_scroll_animation();
        }
        let (generation, fetch_signal) = {
            let Some(tab) = self.tabs.get_mut(id) else {
                return;
            };
            tab.document_fetch.abort();
            tab.document_fetch = FetchController::new();
            if let (Some(session), Some(document)) =
                (tab.renderer_session.as_ref(), tab.renderer_document)
            {
                let _ = session.cancel_document(document);
            }
            tab.pending_renderer_page = None;
            tab.renderer_document = None;
            tab.renderer_input_sequence = 0;
            tab.renderer_input_poll_budget = 0;
            tab.pending_renderer_scroll = None;
            tab.renderer_revision = 0;
            tab.renderer_load_metrics = None;
            tab.page_diagnostics = Default::default();
            tab.renderer_next_timer = None;
            tab.renderer_runtime_clock = None;
            tab.renderer_work_pending = false;
            tab.last_scroll_activity = None;
            tab.performance = TabPerformance::default();
            tab.scroll_animation = Default::default();
            if tab.loading {
                tab.generation = tab.generation.wrapping_add(1);
            }
            match history_mode {
                HistoryMode::Push => {
                    tab.script_navigation.reset(&url);
                    if tab.history.get(tab.history_index) != Some(&url) {
                        if !tab.history.is_empty() {
                            tab.history.truncate(tab.history_index + 1);
                        }
                        tab.history.push(url.clone());
                        tab.history_index = tab.history.len() - 1;
                    }
                }
                HistoryMode::Existing => tab.script_navigation.reset(&url),
                HistoryMode::Script => {}
            }
            tab.generation = tab.generation.wrapping_add(1);
            tab.loading = true;
            tab.crashed = false;
            tab.omnibox_text.clone_from(&url);
            tab.title.clone_from(&url);
            tab.status_text = format!("Loading {url} …");
            (tab.generation, tab.document_fetch.signal())
        };
        self.start_renderer_for(id);
        self.ensure_renderer_monitoring();
        self.update_renderer_tab_title(id, &url);
        if is_active {
            self.update_active_tab_title(&url);
            KillTimer(self.window, ID_RENDERER_RUNTIME_TIMER);
            self.update_history_buttons();
            if let Some(benchmark) = self.benchmark.as_mut()
                && benchmark.navigation_started.is_none()
            {
                benchmark.navigation_started = Some(Instant::now());
            }
            set_window_text(self.controls.address, &url);
            self.set_status(&format!("Loading {url} …"));
        } else {
            InvalidateRect(self.window, null(), 0);
        }

        let tab_router = self.app.tab_router.clone();
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
                    let response = fetch_navigation(&client, &url, &fetch_signal)?;
                    let network_time = started.elapsed();
                    let bytes = response.body.len() as u64;
                    let final_url = response.final_url().as_str().to_string();
                    let status = response.status;
                    let content_type = response.content_type().unwrap_or_default().to_string();
                    let body = response.body.into_bytes();
                    metrics.record_success(bytes, 0);
                    Ok(LoadedPage {
                        body,
                        final_url,
                        status,
                        content_type,
                        bytes,
                        network_time,
                    })
                })();
                if result.is_err() {
                    metrics.record_failure();
                }
                let message = Box::new(LoadMessage { generation, result });
                let pointer = Box::into_raw(message);
                let posted = tab_router.destination(id).is_some_and(|window| unsafe {
                    PostMessageW(
                        window as Hwnd,
                        WM_APP_PAGE_LOADED,
                        id.get() as usize,
                        pointer as isize,
                    ) != 0
                });
                if !posted {
                    unsafe {
                        drop(Box::from_raw(pointer));
                    }
                }
            });
        if let Err(error) = navigation_thread {
            if let Some(tab) = self.tabs.get_mut(id) {
                tab.loading = false;
                tab.status_text = format!("Could not start navigation: {error}");
            }
            if is_active {
                self.set_status(&format!("Could not start navigation: {error}"));
            }
        }
    }

    pub(super) unsafe fn go_back(&mut self) {
        if self.history_index > 0 {
            self.history_index -= 1;
            let url = self.history[self.history_index].clone();
            self.begin_navigation(url, HistoryMode::Existing);
        }
    }

    pub(super) unsafe fn go_forward(&mut self) {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            let url = self.history[self.history_index].clone();
            self.begin_navigation(url, HistoryMode::Existing);
        }
    }

    pub(super) unsafe fn reload(&mut self) {
        self.start_renderer();
        if let Some(url) = self.history.get(self.history_index).cloned() {
            self.begin_navigation(url, HistoryMode::Existing);
        }
    }

    pub(super) unsafe fn update_history_buttons(&self) {
        EnableWindow(self.controls.back, (self.history_index > 0) as i32);
        EnableWindow(
            self.controls.forward,
            (self.history_index + 1 < self.history.len()) as i32,
        );
        EnableWindow(self.controls.reload, (!self.history.is_empty()) as i32);
    }
}

fn fetch_navigation(
    client: &winhttp::HttpClient,
    url: &str,
    signal: &FetchSignal,
) -> Result<winhttp::HttpResponse, String> {
    let request = FetchRequest::navigation(url)
        .map_err(|error| error.to_string())?
        .with_signal(signal.clone());
    client.fetch(request).map_err(|error| error.to_string())
}
