//! Omnibox navigation, history traversal, and network-load dispatch.

use super::tabs::TabId;
use super::*;
mod fetch;
mod history;
mod submission;
use better_web_browser::fetch::{FetchController, Origin};
use better_web_browser::navigation::request::FormPost;
use fetch::fetch_navigation;

#[derive(Clone, Copy, Debug)]
pub(super) enum HistoryMode {
    Push,
    Existing,
    Script,
    ScriptPush,
    Replace,
    Recovery,
}

impl BrowserState {
    pub(super) fn approved_navigation_target(&self, input: &str) -> Result<String, &'static str> {
        let parsed = url::Url::parse(input).map_err(|_| "Invalid navigation URL")?;
        if matches!(parsed.scheme(), "http" | "https") {
            return Ok(input.to_owned());
        }
        self.app
            .protocol_handlers
            .borrow()
            .navigate(input)
            .ok_or("No approved handler for this URL scheme")
    }

    pub(super) unsafe fn open_url_in_new_tab(&mut self, url: String, foreground: bool) {
        match self.approved_navigation_target(&url) {
            Ok(target) => self.add_tab(Some(target), foreground),
            Err(error) => self.set_status(error),
        }
    }

    pub(super) unsafe fn navigate_from_address(&mut self) {
        let input = window_text(self.controls.address);
        self.navigate_from_input(&input, HistoryMode::Push, true);
    }

    pub(super) unsafe fn navigate_from_input(
        &mut self,
        input: &str,
        history_mode: HistoryMode,
        user_activation: bool,
    ) {
        match normalize_user_input(input) {
            Ok(url) if user_activation => self.begin_user_navigation(url, history_mode),
            Ok(url) => self.begin_navigation(url, history_mode),
            Err(error) => self.set_status(&error.to_string()),
        }
    }

    pub(super) unsafe fn begin_navigation(&mut self, url: String, history_mode: HistoryMode) {
        self.begin_navigation_for_tab(self.tabs.active_id(), url, history_mode, None);
    }

    unsafe fn begin_user_navigation(&mut self, url: String, history_mode: HistoryMode) {
        self.begin_navigation_request_for_tab(
            self.tabs.active_id(),
            url,
            history_mode,
            None,
            None,
            true,
        );
    }

    pub(super) unsafe fn begin_document_navigation(
        &mut self,
        url: String,
        history_mode: HistoryMode,
    ) {
        let id = self.tabs.active_id();
        let tab = self.tabs.active();
        let referrer = tab.current_url().map(str::to_owned);
        let user_activation = matches!(
            (tab.transient_activation, tab.navigation.active_document()),
            (Some((gesture_document, at)), Some(active_document))
                if gesture_document == active_document && at.elapsed() <= Duration::from_secs(5)
        );
        self.begin_navigation_request_for_tab(
            id,
            url,
            history_mode,
            referrer,
            None,
            user_activation,
        );
    }

    pub(super) unsafe fn begin_navigation_for_tab(
        &mut self,
        id: TabId,
        url: String,
        history_mode: HistoryMode,
        referrer: Option<String>,
    ) {
        self.begin_navigation_request_for_tab(id, url, history_mode, referrer, None, false);
    }

    pub(super) unsafe fn begin_navigation_request_for_tab(
        &mut self,
        id: TabId,
        url: String,
        history_mode: HistoryMode,
        referrer: Option<String>,
        post_body: Option<FormPost>,
        user_activation: bool,
    ) {
        let url = match self.approved_navigation_target(&url) {
            Ok(target) => target,
            Err(error) => {
                self.set_status(error);
                return;
            }
        };
        let page_scale = self.page_scale().max(f32::EPSILON);
        let is_active = self.tabs.active_id() == id && !self.processing_background_tab;
        if is_active {
            self.exit_pointer_lock();
            self.exit_page_fullscreen();
        }
        let mut schedule_filmstrip = false;
        if is_active {
            self.cancel_scroll_animation();
            if let Some(document) = self.navigation.active_document()
                && let Some(benchmark) = self.benchmark.as_mut()
            {
                benchmark.wheel_trace.retire_document(document);
            }
        }
        // Audio belongs to the document being replaced, not the next generation.
        self.retire_database_for_tab(id);
        self.retire_speech_for_tab(id);
        self.retire_notifications_for_tab(id);
        self.retire_permissions_for_tab(id);
        self.retire_geolocation_for_tab(id);
        self.retire_media_devices_for_tab(id);
        self.retire_capture_for_document(id);
        self.retire_sensors_for_tab(id);
        self.retire_wake_locks_for_tab(id);
        let (generation, fetch_signal) = {
            let Some(tab) = self.tabs.get_mut(id) else {
                return;
            };
            tab.incidents.navigations = tab.incidents.navigations.saturating_add(1);
            tab.incidents
                .record("navigation", format!("begin {history_mode:?}: {url}"));
            tab.document_fetch.abort();
            tab.renderer_websockets.cancel_all();
            tab.document_fetch = FetchController::new();
            let generation = match history_mode {
                HistoryMode::Recovery => {
                    let Some(generation) = tab.navigation.begin_recovery() else {
                        return;
                    };
                    generation
                }
                _ => tab.navigation.begin(),
            };
            tab.renderer_input_sequence = 0;
            tab.wheel_gesture = Default::default();
            tab.native_text_generation = 0;
            tab.suppress_page_control_edit = false;
            tab.pointer_cursor_request = None;
            tab.pointer_cursor = better_web_browser::renderer_protocol::PointerCursor::Default;
            tab.renderer_input_poll_budget = 0;
            tab.pending_renderer_inputs.clear();
            tab.pending_text_selections.clear();
            tab.history_traversals.clear();
            tab.renderer_revision = 0;
            tab.video_presentation = Default::default();
            tab.last_renderer_snapshot = None;
            tab.renderer_load_metrics = None;
            tab.page_diagnostics = Default::default();
            tab.renderer_next_timer = None;
            tab.renderer_runtime_clock = None;
            tab.renderer_clock_pending = false;
            tab.renderer_work_pending = false;
            tab.last_scroll_activity = None;
            tab.performance = TabPerformance::default();
            tab.scroll_animation = Default::default();
            // The renderer is replaced for every full navigation. No older entry can retain
            // an identity that would authorize a same-document traversal into that process.
            tab.retire_history_documents();
            if !matches!(history_mode, HistoryMode::Existing) {
                let outgoing_y = tab.scroll_y.max(0) as f32 / page_scale;
                if let Some(entry) = tab.history.get_mut(tab.history_index) {
                    entry.scroll_y = Some(outgoing_y);
                }
            }
            match history_mode {
                HistoryMode::Push | HistoryMode::ScriptPush => {
                    if matches!(history_mode, HistoryMode::Push) {
                        tab.script_navigation.reset(&url);
                    }
                    if tab.current_url() != Some(url.as_str()) {
                        tab.push_history_entry(super::tab_state::HistoryEntry::new(url.clone()));
                    } else {
                        tab.replace_current_history_url(url.clone());
                    }
                }
                HistoryMode::Existing | HistoryMode::Recovery => {
                    tab.script_navigation.reset(&url);
                }
                HistoryMode::Script | HistoryMode::Replace => {
                    if matches!(history_mode, HistoryMode::Replace) {
                        tab.script_navigation.reset(&url);
                    }
                    tab.replace_current_history_url(url.clone());
                }
            }
            tab.pending_history_scroll_y =
                matches!(history_mode, HistoryMode::Existing | HistoryMode::Recovery)
                    .then(|| tab.history.get(tab.history_index))
                    .flatten()
                    .filter(|entry| {
                        entry.scroll_restoration
                            == better_web_browser::renderer_protocol::ScrollRestorationMode::Auto
                    })
                    .and_then(|entry| entry.scroll_y);
            tab.crashed = false;
            tab.omnibox_text.clone_from(&url);
            tab.title.clone_from(&url);
            tab.status_text = format!("Loading {url} …");
            (generation, tab.document_fetch.signal())
        };
        // A full document navigation is a renderer ownership boundary. The previous page may be
        // inside uninterruptible script, layout, or IPC work, so do not let its process delay the
        // replacement document or carry stale queued work across the navigation.
        self.replace_renderer_for_navigation(id);
        self.start_renderer_for(id);
        self.ensure_renderer_monitoring();
        self.update_renderer_tab_title(id, &url);
        if is_active {
            self.apply_current_pointer_cursor();
            self.update_active_tab_title(&url);
            KillTimer(self.window, ID_RENDERER_RUNTIME_TIMER);
            self.update_history_buttons();
            if let Some(benchmark) = self.benchmark.as_mut()
                && benchmark.navigation_started.is_none()
            {
                benchmark.navigation_started = Some(Instant::now());
                schedule_filmstrip = true;
            }
            set_window_text(self.controls.address, &url);
            self.set_status(&format!("Loading {url} …"));
        } else {
            InvalidateRect(self.window, null(), 0);
        }
        if schedule_filmstrip {
            self.schedule_benchmark_filmstrip();
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
                let post = |result| {
                    let pointer = Box::into_raw(Box::new(LoadMessage { generation, result }));
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
                    posted
                };
                let stream = better_web_browser::renderer_process::NavigationBody::default();
                let result =
                    (|| -> Result<super::document_activation::NavigationResult, String> {
                        let client = http_client;
                        let mut response = fetch_navigation(
                            &client,
                            &url,
                            &fetch_signal,
                            referrer.as_deref(),
                            post_body,
                            user_activation,
                        )?;
                        let network_time = started.elapsed();
                        let fetched_url = response
                            .url_list
                            .last()
                            .ok_or("navigation response has no URL")?
                            .as_str();
                        let redirected = response.url_list.len() > 1;
                        let final_url = fetch::response_document_url(&url, fetched_url, redirected);
                        let status = response.status;
                        let content_type = response
                            .headers
                            .get("content-type")
                            .unwrap_or_default()
                            .to_string();
                        let policy = std::sync::Arc::new(
                            better_web_browser::fetch::csp::PolicyContainer::from_headers(
                                &final_url,
                                &response.headers,
                            )
                            .map_err(|error| error.to_string())?,
                        );
                        let screen_wake_lock_allowed =
                            super::wake_lock::screen_wake_lock_allowed(&response.headers);
                        if !post(Ok(super::document_activation::NavigationResult::Headers(
                            LoadedPage {
                                stream: Some(stream.clone()),
                                body: Vec::new(),
                                final_url,
                                redirected,
                                status,
                                content_type,
                                policy,
                                screen_wake_lock_allowed,
                                bytes: 0,
                                network_time,
                            },
                        ))) {
                            return Err("navigation destination closed".into());
                        }
                        let mut bytes = 0;
                        while let Some(chunk) =
                            response.next_chunk().map_err(|error| error.to_string())?
                        {
                            bytes += chunk.len() as u64;
                            stream.append(&chunk)?;
                        }
                        stream.finish(Ok(()));
                        metrics.record_success(bytes, 0);
                        Ok(super::document_activation::NavigationResult::Complete {
                            bytes,
                            network_time: started.elapsed(),
                        })
                    })();
                if let Err(error) = &result {
                    stream.finish(Err(error.clone()));
                    metrics.record_failure();
                }
                post(result);
            });
        if let Err(error) = navigation_thread {
            if let Some(tab) = self.tabs.get_mut(id) {
                tab.navigation.fail();
                tab.status_text = format!("Could not start navigation: {error}");
                tab.incidents
                    .record("navigation", format!("thread start failed: {error}"));
            }
            if is_active {
                self.set_status(&format!("Could not start navigation: {error}"));
            }
        }
    }
}
