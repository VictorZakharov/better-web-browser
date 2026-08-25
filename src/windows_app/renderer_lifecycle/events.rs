use super::*;
use better_web_browser::renderer_process::{RendererEvent, RendererState};
use better_web_browser::renderer_protocol::{NavigationCause, NavigationDisposition};
use std::sync::Arc;

impl BrowserState {
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
                    self.update_renderer_status(id, &title, |status| {
                        status.last_diagnostic = Some(format!("{code}: {text}"));
                    });
                }
                RendererEvent::Unresponsive => {
                    self.update_renderer_status(id, &title, |status| {
                        status.phase = RendererLifecyclePhase::Unresponsive;
                    });
                }
                RendererEvent::FetchBatch { document, requests } => {
                    self.begin_renderer_fetch_batch(id, document, requests);
                }
                RendererEvent::Presentation(presentation) => {
                    self.process_for_tab(id, |state| {
                        state.activate_renderer_presentation(*presentation)
                    });
                }
                RendererEvent::TimeAdvanced {
                    document,
                    next_timer_micros,
                } => {
                    self.process_for_tab(id, |state| {
                        state.complete_renderer_time_advance(document, next_timer_micros)
                    });
                }
                RendererEvent::DocumentFailed { document, detail } => {
                    let current = self
                        .tabs
                        .get_mut(id)
                        .is_some_and(|tab| tab.renderer_document == Some(document));
                    if current {
                        self.contain_page_engine_failure(id, detail);
                    }
                }
                RendererEvent::NavigationRequested {
                    document,
                    url,
                    disposition,
                    cause,
                } => {
                    self.process_for_tab(id, |state| {
                        if state.renderer_document != Some(document) {
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
                RendererEvent::Exited(renderer_exit) => exit = Some(renderer_exit),
            }
        }

        if let Some(exit) = exit {
            let crash_surface = exit.crash_surface();
            self.update_renderer_status(id, &title, |status| {
                status.phase = RendererLifecyclePhase::Exited;
                status.last_exit = Some(exit);
            });
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
            (tab.renderer_document == Some(document))
                .then(|| {
                    tab.renderer_session.as_ref().map(|session| {
                        (
                            tab.reader_url.clone(),
                            tab.document_fetch.signal(),
                            session.fetch_response_sink(document),
                        )
                    })
                })
                .flatten()
        });
        let Some((document_url, signal, sink)) = context else {
            return;
        };
        let result = renderer_fetch::spawn_fetch_batch(renderer_fetch::RendererFetchBatch {
            tab_id: id,
            document,
            document_url,
            requests,
            client: Arc::clone(&self.http_client),
            signal,
            sink,
            tab_router: self.app.tab_router.clone(),
        });
        if let Err(error) = result {
            self.contain_page_engine_failure(id, error);
        }
    }
}
