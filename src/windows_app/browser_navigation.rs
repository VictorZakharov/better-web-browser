//! Omnibox navigation, history traversal, and network-load dispatch.

use super::resources::{ResourceLoadContext, load_page_resources};
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
            if let Some(mut runtime) = tab.script_runtime.take() {
                runtime.cancel_document();
            }
            tab.script_runtime_clock = None;
            tab.post_load_script_not_before = None;
            tab.pending_async_scripts.clear();
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
        self.update_renderer_tab_title(id, &url);
        if is_active {
            self.update_active_tab_title(&url);
            KillTimer(self.window, ID_SCRIPT_RUNTIME_TIMER);
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
                    let mut response = fetch_navigation(&client, &url, &fetch_signal)?;
                    let mut network_time = started.elapsed();
                    let mut bytes = response.body.len() as u64;
                    let mut resource_budget = PAGE_RESOURCE_BUDGET;
                    let mut visited = HashSet::from([response.final_url().as_str().to_string()]);
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
                        let final_url = response.final_url().as_str().to_string();
                        let status = u32::from(response.status);
                        let decoded = winhttp::decode_document(
                            response.body.as_bytes(),
                            response.content_type(),
                        );
                        let html = decoded.text;
                        let html_parse_started = Instant::now();
                        let mut rendered_page = Page::parse_scripted(&html, &final_url);
                        rendered_page.character_set = decoded.encoding.to_string();
                        html_parse_time += html_parse_started.elapsed();

                        if navigation_count < 5
                            && let Some(refresh_url) = rendered_page.immediate_refresh_url()
                            && visited.insert(refresh_url.clone())
                        {
                            let refresh_started = Instant::now();
                            let refresh_response =
                                fetch_navigation(&client, &refresh_url, &fetch_signal);
                            network_time += refresh_started.elapsed();
                            if let Ok(next_response) = refresh_response {
                                bytes += next_response.body.len() as u64;
                                visited.insert(next_response.final_url().as_str().to_string());
                                response = next_response;
                                navigation_count += 1;
                                continue;
                            }
                        }

                        let mut loaded_resources = HashSet::new();
                        load_page_resources(
                            &mut rendered_page,
                            ResourceLoadContext {
                                client: &client,
                                signal: &fetch_signal,
                                loaded: &mut loaded_resources,
                                resource_budget: &mut resource_budget,
                                bytes: &mut bytes,
                                network_time: &mut network_time,
                                processing_time: &mut resource_processing_time,
                            },
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
