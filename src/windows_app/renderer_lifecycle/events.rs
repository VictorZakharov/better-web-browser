use super::*;
use crate::windows_app::navigation_transaction::PresentationDeadline;
use better_web_browser::renderer_process::{RendererEvent, RendererExitReason, RendererState};
use better_web_browser::renderer_protocol::{NavigationCause, NavigationDisposition};
use std::sync::Arc;

impl BrowserState {
    pub(super) unsafe fn enforce_first_presentation_deadline(&mut self, id: TabId) {
        let action = self
            .tabs
            .get_mut(id)
            .and_then(|tab| tab.navigation.deadline(Instant::now()));
        match action {
            Some(PresentationDeadline::Retry) => {
                if self.tabs.active_id() == id {
                    self.set_status("Renderer did not present the document; retrying once …");
                }
                self.replace_renderer_for_navigation(id);
                self.start_renderer_for(id);
                self.ensure_renderer_monitoring();
            }
            Some(PresentationDeadline::Failed) => self.contain_page_engine_failure(
                id,
                "renderer did not produce a first presentation after a clean retry".into(),
            ),
            None => {}
        }
    }

    pub(super) unsafe fn poll_renderer(&mut self, id: TabId) {
        self.flush_renderer_inputs_for(id);
        if let Some(tab) = self.tabs.get_mut(id) {
            tab.renderer_input_poll_budget = tab.renderer_input_poll_budget.saturating_sub(1);
        }
        let snapshot_and_events = self.tabs.get_mut(id).and_then(|tab| {
            tab.renderer_session.as_ref().map(|session| {
                let snapshot = session.snapshot();
                let mut events = Vec::new();
                while let Ok(Some(event)) = session.try_event() {
                    events.push(event);
                }
                (tab.title.clone(), snapshot, events)
            })
        });
        let Some((title, snapshot, events)) = snapshot_and_events else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(id) {
            tab.last_renderer_snapshot = Some(snapshot.clone());
        }
        let mut exit = snapshot.exit.clone();
        self.update_renderer_status(id, &title, |status| {
            status.phase = match snapshot.state {
                RendererState::Running => RendererLifecyclePhase::Running,
                RendererState::Unresponsive => RendererLifecyclePhase::Unresponsive,
                RendererState::Exited => RendererLifecyclePhase::Exited,
            };
            status.snapshot = Some(snapshot);
        });

        for event in events {
            match event {
                RendererEvent::Diagnostic { code, text } => {
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.incidents
                            .record("renderer", format!("diagnostic {code}: {text}"));
                    }
                    self.update_renderer_status(id, &title, |status| {
                        status.last_diagnostic = Some(format!("{code}: {text}"));
                    });
                }
                RendererEvent::Unresponsive => {
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.incidents.record("renderer", "became unresponsive");
                    }
                    self.update_renderer_status(id, &title, |status| {
                        status.phase = RendererLifecyclePhase::Unresponsive;
                    });
                }
                RendererEvent::FetchBatch { document, requests } => {
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.incidents.fetch_batches = tab.incidents.fetch_batches.saturating_add(1);
                        tab.incidents.record(
                            "fetch",
                            format!(
                                "batch for document {}: {} requests",
                                document.get(),
                                requests.len()
                            ),
                        );
                    }
                    self.begin_renderer_fetch_batch(id, document, requests);
                }
                RendererEvent::FetchAbort {
                    document,
                    request_id,
                } => {
                    if let Some(tab) = self.tabs.get_mut(id)
                        && tab.navigation.owns_document(document)
                    {
                        tab.renderer_fetches.abort(document, request_id);
                    }
                }
                RendererEvent::Presentation(presentation) => {
                    self.process_for_tab(id, |state| {
                        state.activate_renderer_presentation(*presentation)
                    });
                }
                RendererEvent::RuntimeUpdate(update) => {
                    self.process_for_tab(id, |state| {
                        state.complete_renderer_runtime_update(*update)
                    });
                }
                RendererEvent::DocumentFailed { document, detail } => {
                    let current = self
                        .tabs
                        .get_mut(id)
                        .is_some_and(|tab| tab.navigation.owns_document(document));
                    if current {
                        self.abandon_page_fullscreen_owned_by(id);
                        self.contain_page_engine_failure(id, detail);
                    }
                }
                RendererEvent::NavigationRequested {
                    document,
                    url,
                    disposition,
                    cause,
                } => {
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.incidents
                            .record("renderer-nav", format!("{cause:?}/{disposition:?}: {url}"));
                    }
                    self.process_for_tab(id, |state| {
                        if !state.navigation.owns_document(document) {
                            return;
                        }
                        match disposition {
                            NavigationDisposition::CurrentTab
                                if cause == NavigationCause::UserActivation =>
                            {
                                state.begin_navigation(url, browser_navigation::HistoryMode::Push)
                            }
                            NavigationDisposition::CurrentTab
                                if state.allow_script_navigation(&url) =>
                            {
                                state.begin_navigation(url, browser_navigation::HistoryMode::Script)
                            }
                            NavigationDisposition::NewForegroundTab => {
                                state.open_url_in_new_tab(url, true)
                            }
                            NavigationDisposition::NewBackgroundTab => {
                                state.open_url_in_new_tab(url, false)
                            }
                            NavigationDisposition::CurrentTab => {}
                        }
                    });
                }
                RendererEvent::PointerCursor(result) => {
                    self.process_for_tab(id, |state| state.apply_renderer_pointer_cursor(result));
                }
                RendererEvent::FullscreenRequested(request) => {
                    self.handle_fullscreen_request(id, request);
                }
                RendererEvent::CookieMutation(mutation) => {
                    let mut correction_error = None;
                    self.process_for_tab(id, |state| {
                        correction_error = state.apply_renderer_cookie_mutation(mutation).err();
                    });
                    if let Some(error) = correction_error {
                        self.contain_page_engine_failure(id, error);
                    }
                }
                RendererEvent::StorageMutation(request) => {
                    let mut correction_error = None;
                    self.process_for_tab(id, |state| {
                        correction_error = state.apply_renderer_storage_mutation(request).err();
                    });
                    if let Some(error) = correction_error {
                        self.contain_page_engine_failure(id, error);
                    }
                }
                RendererEvent::Exited(renderer_exit) => {
                    self.abandon_page_fullscreen_owned_by(id);
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.incidents.record(
                            "renderer",
                            format!(
                                "process {} exited {:#x}: {:?}",
                                renderer_exit.process_id, renderer_exit.code, renderer_exit.reason
                            ),
                        );
                    }
                    exit = Some(renderer_exit);
                }
            }
        }

        if let Some(exit) = exit {
            let crash_surface = exit.crash_surface();
            let task_budget_exceeded =
                matches!(exit.reason, RendererExitReason::TaskBudgetExceeded(_));
            self.update_renderer_status(id, &title, |status| {
                status.phase = RendererLifecyclePhase::Exited;
                status.last_exit = Some(exit);
            });
            if task_budget_exceeded {
                let recovery_url = self
                    .tabs
                    .get_mut(id)
                    .and_then(|tab| tab.current_url().map(str::to_owned));
                if let Some(url) = recovery_url
                    && self
                        .tabs
                        .get_mut(id)
                        .is_some_and(|tab| !tab.navigation.is_loading())
                {
                    if self.tabs.active_id() == id {
                        self.set_status("Renderer stopped responding; reloading once …");
                    }
                    self.begin_navigation_for_tab(
                        id,
                        url,
                        browser_navigation::HistoryMode::Recovery,
                    );
                    if self
                        .tabs
                        .get_mut(id)
                        .is_some_and(|tab| tab.navigation.is_loading())
                    {
                        return;
                    }
                }
            }
            let recovery = self.tabs.get_mut(id).and_then(|tab| {
                let recovery = tab.navigation.renderer_exited();
                if recovery.is_some() {
                    tab.renderer_session.take();
                    tab.renderer_clock_pending = false;
                    tab.renderer_work_pending = false;
                    tab.pointer_cursor_request = None;
                    tab.pointer_cursor =
                        better_web_browser::renderer_protocol::PointerCursor::Default;
                }
                recovery
            });
            match recovery {
                Some(PresentationDeadline::Retry) => {
                    if self.tabs.active_id() == id {
                        self.apply_current_pointer_cursor();
                        self.set_status("Renderer exited before first paint; retrying once …");
                    }
                    self.start_renderer_for(id);
                    self.ensure_renderer_monitoring();
                    return;
                }
                Some(PresentationDeadline::Failed) => {
                    let detail = crash_surface
                        .as_ref()
                        .map(|surface| {
                            format!(
                                "renderer exited before first paint after a clean retry: {}",
                                surface.detail
                            )
                        })
                        .unwrap_or_else(|| {
                            "renderer exited before first paint after a clean retry".into()
                        });
                    self.contain_page_engine_failure(id, detail);
                    return;
                }
                None => {}
            }
            let status = crash_surface.map(|surface| {
                format!(
                    "{}: {}. Reload to restart the renderer.",
                    surface.title, surface.detail
                )
            });
            if let Some(tab) = self.tabs.get_mut(id) {
                if let Some(status) = status.as_ref() {
                    tab.mark_crashed(status.clone());
                } else {
                    tab.renderer_session.take();
                }
                tab.pointer_cursor_request = None;
                tab.pointer_cursor = better_web_browser::renderer_protocol::PointerCursor::Default;
            }
            if self.tabs.active_id() == id {
                self.apply_current_pointer_cursor();
            }
            if self.tabs.active_id() == id
                && let Some(status) = status
            {
                self.set_status(&status);
                self.refresh_accessibility_full();
            }
        }
    }

    unsafe fn begin_renderer_fetch_batch(
        &mut self,
        id: TabId,
        document: better_web_browser::renderer_protocol::DocumentId,
        requests: Vec<better_web_browser::renderer_protocol::RendererFetchRequest>,
    ) {
        let context = self.tabs.get_mut(id).and_then(|tab| {
            tab.navigation
                .owns_document(document)
                .then(|| {
                    tab.renderer_session.as_ref().map(|session| {
                        (
                            tab.reader_url.clone(),
                            tab.document_fetch.signal(),
                            session.fetch_response_sink(document),
                            tab.renderer_fetches.clone(),
                        )
                    })
                })
                .flatten()
        });
        let Some((document_url, signal, sink, registry)) = context else {
            return;
        };
        let result = renderer_fetch::spawn_fetch_batch(renderer_fetch::RendererFetchBatch {
            tab_id: id,
            document,
            document_url,
            requests,
            client: Arc::clone(&self.http_client),
            signal,
            registry,
            sink,
            tab_router: self.app.tab_router.clone(),
        });
        if let Err(error) = result {
            self.contain_page_engine_failure(id, error);
        }
    }
}
