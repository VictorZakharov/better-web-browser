use super::*;
use crate::windows_app::navigation_transaction::PresentationDeadline;
use better_web_browser::renderer_process::{RendererEvent, RendererState};
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
        if let Err(error) = self.pump_broadcast_deliveries(id) {
            self.contain_page_engine_failure(id, error);
        }
        if let Err(error) = self.pump_storage_updates(id) {
            self.contain_page_engine_failure(id, error);
        }
        self.flush_renderer_inputs_for(id);
        if let Some(tab) = self.tabs.get_mut(id) {
            tab.renderer_input_poll_budget = tab.renderer_input_poll_budget.saturating_sub(1);
        }
        let snapshot_and_events = self.tabs.get_mut(id).and_then(|tab| {
            tab.renderer_session.as_ref().map(|session| {
                let snapshot = session.snapshot();
                let events = if tab.deferred_renderer_events.is_empty() {
                    super::event_batch::collect(|accepts| {
                        session.try_event_if(accepts).ok().flatten()
                    })
                } else {
                    tab.deferred_renderer_events.drain(..).collect()
                };
                (
                    tab.title.clone(),
                    snapshot,
                    events,
                    session.pending_events() > 0,
                )
            })
        });
        let Some((title, snapshot, events, remaining)) = snapshot_and_events else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(id) {
            tab.last_renderer_snapshot = Some(snapshot.clone());
        }
        let session_id = snapshot.session_id;
        // Preserve FIFO delivery before acting on a terminal diagnostic snapshot.
        let mut exit = snapshot.exit.clone().filter(|_| !remaining);
        self.update_renderer_status(id, &title, |status| {
            status.phase = match snapshot.state {
                RendererState::Running => RendererLifecyclePhase::Running,
                RendererState::Unresponsive => RendererLifecyclePhase::Unresponsive,
                RendererState::Exited => RendererLifecyclePhase::Exited,
            };
            status.snapshot = Some(snapshot);
        });

        let mut events = events.into_iter().peekable();
        let turn = super::turn_budget::RendererTurnBudget::new(Instant::now());
        while let Some(event) = events.next() {
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
                RendererEvent::WebSocketCommand(command) => {
                    self.handle_websocket_command(id, command);
                }
                RendererEvent::DatabaseCommand(command) => {
                    self.handle_database_command(id, command);
                }
                RendererEvent::SpeechRequest(request) => {
                    self.handle_speech_request(id, request);
                }
                RendererEvent::NotificationRequest(request) => {
                    self.handle_notification_request(id, request);
                }
                RendererEvent::ProtocolHandlerRequest(request) => {
                    self.handle_protocol_handler_request(id, request);
                }
                RendererEvent::PermissionRequest(request) => {
                    self.handle_permission_request(id, request);
                }
                RendererEvent::GeolocationRequest(request) => {
                    self.handle_geolocation_request(id, request);
                }
                RendererEvent::MediaDeviceRequest(request) => {
                    self.handle_media_device_request(id, request);
                }
                RendererEvent::MediaCaptureRequest(request) => {
                    self.handle_media_capture_request(id, request);
                }
                RendererEvent::SensorRequest(request) => {
                    self.handle_sensor_request(id, request);
                }
                RendererEvent::ClipboardRequest(request) => {
                    self.handle_clipboard_request(id, request);
                }
                RendererEvent::FilePickerRequest(request) => {
                    self.handle_file_picker_request(id, request);
                }
                RendererEvent::TextSelectionUpdate(update) => {
                    self.process_for_tab(id, |state| state.apply_text_selection_update(update));
                }
                RendererEvent::Presentation(presentation) => {
                    self.process_for_tab(id, |state| {
                        state.activate_renderer_presentation(*presentation)
                    });
                }
                RendererEvent::VideoFrame(update) => {
                    self.process_for_tab(id, |state| state.activate_video_frame(*update));
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
                        self.abandon_pointer_lock_owned_by(id);
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
                                state.begin_document_navigation(
                                    url,
                                    browser_navigation::HistoryMode::Push,
                                )
                            }
                            NavigationDisposition::CurrentTab
                                if state.allow_script_navigation(&url) =>
                            {
                                state.begin_document_navigation(
                                    url,
                                    browser_navigation::HistoryMode::Script,
                                )
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
                RendererEvent::PointerLockRequested(request) => {
                    self.handle_pointer_lock_request(id, request);
                }
                RendererEvent::WakeLockRequested(request) => {
                    self.handle_wake_lock_request(id, request);
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
                RendererEvent::PolicyMutation(mutation) => {
                    let mut policy_error = None;
                    self.process_for_tab(id, |state| {
                        policy_error = state.renderer_fetches.append_meta_policy(mutation).err();
                    });
                    if let Some(error) = policy_error {
                        self.contain_page_engine_failure(id, error);
                    }
                }
                RendererEvent::StorageMutation(request) => {
                    // Only combine adjacent intents in this bounded UI turn. Navigation,
                    // presentation, and other-area events remain ordering barriers.
                    let requests = super::event_batch::storage_transaction(request, &mut events);
                    let mut applied = Ok(true);
                    self.process_for_tab(id, |state| {
                        applied = state.apply_renderer_storage_mutations(&requests);
                    });
                    match applied {
                        Ok(false) => {
                            self.defer_renderer_event_turn(
                                id,
                                session_id,
                                requests
                                    .into_iter()
                                    .map(RendererEvent::StorageMutation)
                                    .chain(events),
                            );
                            exit = None;
                            break;
                        }
                        Err(error) => self.contain_page_engine_failure(id, error),
                        Ok(true) => {}
                    }
                }
                RendererEvent::BroadcastCommand(command) => {
                    let mut applied = Ok(true);
                    self.process_for_tab(id, |state| {
                        applied = state.apply_broadcast_command(command.clone());
                    });
                    match applied {
                        Ok(false) => {
                            self.defer_renderer_event_turn(
                                id,
                                session_id,
                                std::iter::once(RendererEvent::BroadcastCommand(command))
                                    .chain(events),
                            );
                            exit = None;
                            break;
                        }
                        Err(error) => self.contain_page_engine_failure(id, error),
                        Ok(true) => {}
                    }
                }
                RendererEvent::Exited(renderer_exit) => {
                    self.abandon_pointer_lock_owned_by(id);
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
            if turn.should_yield(
                events.peek().is_some(),
                Instant::now(),
                crate::windows_app::message_pump::input_is_waiting,
            ) {
                self.defer_renderer_event_turn(id, session_id, events);
                // A diagnostic terminal snapshot cannot overtake deferred FIFO
                // output, including its original terminal event.
                exit = None;
                break;
            }
        }

        let Some(remaining) = self.finish_renderer_event_turn(id, session_id) else {
            // Navigation or error handling may have replaced the session while
            // consuming this batch. Neither re-arm nor apply its exit to the new one.
            return;
        };

        if !remaining && let Some(exit) = exit {
            self.finish_renderer_exit(id, &title, exit);
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
