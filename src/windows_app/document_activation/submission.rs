//! Initial document state and isolated-renderer submission.
use super::*;

impl BrowserState {
    pub(super) unsafe fn submit_pending_renderer_document(&mut self) {
        let Some(page) = self.navigation.page_for_submission() else {
            return;
        };
        let Some(session) = self.renderer_session.as_ref() else {
            self.start_renderer_for(self.id);
            return;
        };
        let document = match self.navigation.document_id() {
            Ok(document) => document,
            Err(error) => {
                self.navigation.fail();
                self.set_status(&format!("Renderer document rejected: {error}"));
                return;
            }
        };
        if let Err(error) =
            self.renderer_fetches
                .install_root(document, &page.final_url, Arc::clone(&page.policy))
        {
            self.navigation.fail();
            self.set_status(&format!("Could not install document policy: {error}"));
            return;
        }
        let body_length = match u32::try_from(page.body.len()) {
            Ok(length) => length,
            Err(_) => {
                self.navigation.fail();
                self.set_status("Renderer document exceeded the IPC byte limit");
                return;
            }
        };
        let start = DocumentStart {
            document,
            url: page.final_url.clone(),
            status: page.status,
            content_type: page.content_type,
            diagnostic_selectors: self
                .benchmark
                .as_ref()
                .map(|benchmark| benchmark.diagnostic_selectors.clone())
                .unwrap_or_default(),
            body_length,
            viewport: self.renderer_viewport(),
            prefers_dark_color_scheme: self.app.prefers_dark_color_scheme.get(),
        };
        let (state, storage_subscription) = match (
            self.http_client.document_cookie_snapshot(&page.final_url),
            self.app
                .storage_coordinator
                .subscribe(&page.final_url)
                .map_err(|error| error.to_string()),
            self.session_storage
                .snapshot(&page.final_url)
                .map_err(|error| error.to_string()),
        ) {
            (Ok(cookie), Ok((local_storage, subscription)), Ok(session_storage)) => (
                DocumentState {
                    cookie_version: cookie.version,
                    cookie_header: cookie.header,
                    local_storage,
                    session_storage,
                },
                subscription,
            ),
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                self.navigation.fail();
                self.set_status(&format!("Could not prepare document state: {error}"));
                return;
            }
        };
        let metrics = RendererLoadMetrics {
            final_url: page.final_url,
            status: page.status,
            bytes: page.bytes,
            network_time: page.network_time,
        };
        let submission = if let Some(stream) = page.stream {
            session.load_streaming_document(start, state, stream)
        } else {
            session.load_document(start, state, page.body)
        };
        match submission {
            Ok(()) => {
                self.watch_storage_events(self.id, session, &storage_subscription);
                self.storage_subscription = Some((document, storage_subscription));
                if !self.navigation.document_submitted(document, Instant::now()) {
                    return;
                }
                self.reader_url.clone_from(&metrics.final_url);
                self.renderer_input_sequence = 0;
                self.pointer_cursor_request = None;
                self.pointer_cursor = better_web_browser::renderer_protocol::PointerCursor::Default;
                self.renderer_input_poll_budget = 0;
                self.pending_renderer_inputs.clear();
                self.renderer_revision = 0;
                self.renderer_load_metrics = Some(metrics);
                self.renderer_next_timer = None;
                self.renderer_runtime_clock = Some(Instant::now());
                self.renderer_clock_pending = false;
                self.renderer_work_pending = true;
                self.record_renderer_submission(document, body_length);
                self.status_text = "Rendering in the isolated page process …".into();
                if !self.processing_background_tab {
                    self.set_status("Rendering in the isolated page process …");
                } else {
                    self.route_renderer_lifecycle(
                        better_web_browser::renderer_protocol::DocumentLifecycle::Hidden,
                    );
                }
            }
            Err(error) => {
                self.navigation.fail();
                self.contain_page_engine_failure(
                    self.id,
                    format!("could not transfer the document to its renderer: {error}"),
                );
            }
        }
    }
}
