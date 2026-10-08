//! Initial document decoding, speculative loading, script startup, and first presentation.

use super::*;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn load(
        start: DocumentStart,
        state: DocumentState,
        body: Vec<u8>,
        connection: &mut ChildConnection,
        text: RendererTextSystem,
    ) -> Result<LoadResult, String> {
        let parse_started = Instant::now();
        let decoded = crate::winhttp::decode_document(
            &body,
            (!start.content_type.is_empty()).then_some(start.content_type.as_str()),
        );
        Self::load_source(start, state, decoded, None, connection, text, parse_started)
    }

    pub(super) fn load_source(
        start: DocumentStart,
        state: DocumentState,
        mut decoded: crate::winhttp::DecodedText,
        mut navigation: Option<navigation::StreamingInput>,
        connection: &mut ChildConnection,
        mut text: RendererTextSystem,
        parse_started: Instant,
    ) -> Result<LoadResult, String> {
        // The HTML Standard permits speculative parsing to start eligible fetches while the
        // authoritative parser continues. Bytes are cached, but execution is admitted only when
        // that parser reaches the actual element. A speculative response cannot prepare a script:
        // https://html.spec.whatwg.org/multipage/parsing.html#speculative-html-parsing
        // A future meta CSP cannot be enforced retroactively against speculative requests.
        // `http-equiv` is an attribute name (character references cannot hide it); this
        // conservative scan may also defer speculation for other pragmas, but never misses
        // a meta CSP that the authoritative parser could recognize.
        let preloads = if !may_speculate_before_parsing(&decoded.text) {
            Default::default()
        } else {
            crate::engine::page::discover_script_preloads(&decoded.text, &start.url)
        };
        let (pending_first_paint, pending_deferred) = start_resource_preloads(
            connection,
            start.document,
            preloads.first_paint,
            preloads.deferred,
        )?;
        let html_parse_started = Instant::now();
        let mut parser = navigation::make_parser(&decoded.text, navigation.as_ref())?;
        let first_step = loop {
            let step = parser.advance();
            if let crate::engine::dom::incremental::ParserStep::Encoding(label) = &step {
                if let Some(input) = navigation.as_mut()
                    && let Some(replay) = input.decoder.change_encoding(label)
                {
                    decoded = replay;
                    input.source.clone_from(&decoded.text);
                    parser = navigation::make_parser(&decoded.text, Some(input))?;
                }
                continue;
            }
            break step;
        };
        let parser_active = !matches!(first_step, crate::engine::dom::incremental::ParserStep::End);
        let reader = crate::document::parse_html(&decoded.text, &start.url);
        let mut page = Page::from_dom(parser.dom().clone(), &start.url);
        page.scripts.clear();
        page.hide_scripted_noscript();
        page.character_set = decoded.encoding.to_string();
        page.set_media_environment(MediaEnvironment::new(
            start.viewport.style_width,
            start.viewport.height,
            start.viewport.dpi as f32 / 96.0,
            start.prefers_dark_color_scheme,
        ));
        page.set_layout_viewport(start.viewport.width, start.viewport.height);
        let html_parse_time = html_parse_started.elapsed();
        if let Some(url) = page.immediate_refresh_url() {
            if let Some(pending) = pending_first_paint {
                discard_resource_preloads(connection, pending)?;
            }
            if let Some(pending) = pending_deferred {
                discard_resource_preloads(connection, pending)?;
            }
            return Ok(LoadResult::Navigate(url, Box::new(text)));
        }

        text.set_dpi(start.viewport.dpi);
        let text = Rc::new(RefCell::new(text));
        let script_layout_page = Rc::new(RefCell::new(page.layout_snapshot()));
        let script_layout_viewport = Rc::new(Cell::new(start.viewport));
        let mut parser_scripts = parser_scripts::ParserScripts::default();
        parser_scripts.set_parsing(parser_active);
        let mut runtime = Self {
            id: start.document,
            status: start.status,
            page,
            reader,
            script_runtime: None,
            viewport: start.viewport,
            text,
            script_layout_page,
            script_layout_viewport,
            layout: Default::default(),
            frame_paint: Vec::new(),
            loaded_resources: HashSet::new(),
            preload_cache: HashMap::new(),
            preload_cache_bytes: 0,
            resource_budget: PAGE_RESOURCE_BUDGET,
            pending_fetches: Vec::new(),
            pending_websockets: Vec::new(),
            pending_databases: Vec::new(),
            pending_speech_requests: Vec::new(),
            pending_notification_requests: Vec::new(),
            pending_protocol_handler_requests: Vec::new(),
            pending_permission_requests: Vec::new(),
            pending_geolocation_requests: Vec::new(),
            pending_media_device_requests: Vec::new(),
            pending_sensor_requests: Vec::new(),
            pending_clipboard_requests: Vec::new(),
            file_pickers: HashMap::new(),
            active_script_fetches: HashMap::new(),
            pending_worker_actions: Vec::new(),
            deferred_network_load: PageLoadReport::default(),
            workers: RendererWorkers::new(!start.diagnostic_selectors.is_empty()),
            parser_scripts,
            navigation,
            parser: parser_active.then_some(parsing::DocumentParser {
                parser,
                next: Some(first_step),
            }),
            pending_dynamic_script_fetch: Vec::new(),
            pending_resource_preloads: pending_deferred.into_iter().collect(),
            resource_render_pending: false,
            resource_event_pending: false,
            resource_style_refresh_pending: false,
            lifecycle: crate::renderer_protocol::DocumentLifecycle::Active,
            accessibility: RendererAccessibility::default(),
            accessibility_selection: None,
            accessibility_values: HashMap::new(),
            focused_node: None,
            pointer_down: [None; 3],
            scroll_drag: None,
            scriptless_pointer_path: Vec::new(),
            last_input_sequence: 0,
            native_text_generation: 0,
            last_acknowledged_revision: 0,
            revision: 0,
            sent_images: HashSet::new(),
            last_served_image_key: None,
            image_budget_warning_sent: false,
            diagnostic_selectors: start.diagnostic_selectors,
            prefers_dark_color_scheme: start.prefers_dark_color_scheme,
            media: None,
            media_captions: HashMap::new(),
            media_activation: Default::default(),
            media_failure: None,
            pending_media_action: None,
            graph_audio_stream: None,
            graph_audio_last_stream_id: 0,
            pending_graph_audio_chunk: None,
            pending_graph_audio_start: None,
            pending_graph_audio_close: None,
            pending_async_outcome: ScriptOutcome::default(),
            resource_events: Default::default(),
            geometry_observers_pending: false,
            resize_observers_pending: false,
            rendering: Default::default(),
            color_paint: Default::default(),
        };
        runtime.record_parser_stylesheets(&[]);

        let resource_started = Instant::now();
        if let Some(pending) = pending_first_paint {
            runtime.pending_resource_preloads.push(pending);
        }
        runtime.start_presentational_preloads(connection)?;
        runtime.sync_script_layout_page();
        let resource_processing_time = resource_started.elapsed();

        let script_started = Instant::now();
        let script_fetch_time = Duration::ZERO;
        let document = runtime.id;
        let mut outcome = ScriptOutcome::default();
        if runtime.parser.is_some() {
            let policy = std::sync::Arc::new(
                crate::fetch::csp::PolicyContainer::from_serialized_policies(
                    &start.url,
                    &start.csp_policies,
                )
                .map_err(|error| error.to_string())?,
            );
            let (mut script_runtime, initial) = runtime
                .page
                .start_parser_runtime(
                    policy,
                    state.cookie_version,
                    &state.cookie_header,
                    state.local_storage,
                    state.session_storage,
                    start.notification_permission,
                    !runtime.diagnostic_selectors.is_empty(),
                    runtime.script_layout_flush_callback(),
                )
                .map_err(|error| error.to_string())?;
            if let Some(state) = runtime
                .navigation
                .as_mut()
                .and_then(|input| input.script_state.take())
            {
                script_runtime.restore_restart_state(state);
            }
            script_runtime.set_history_metrics(
                start.history_length,
                start.history_index,
                start.history_state.as_deref(),
                start.scroll_restoration,
            )?;
            runtime.script_runtime = Some(script_runtime);
            runtime.dispatch_initial_media_selections()?;
            outcome = initial;
            runtime.advance_parser(connection, &mut outcome)?;
            if runtime.encoding_restart_pending() {
                connection.send_state_mutations(
                    document,
                    runtime.last_input_sequence,
                    &mut outcome,
                )?;
                return runtime.restart_encoding(connection);
            }
        }
        connection.send_network_state_updates(document, &mut outcome)?;
        runtime.start_dynamic_script_fetches(connection)?;
        runtime.flush_pending_resource_events()?;
        merge_outcome(
            &mut outcome,
            std::mem::take(&mut runtime.pending_async_outcome),
            runtime.page.dom.document.id(),
        );
        runtime.apply_media_actions(&mut outcome, connection)?;
        runtime.apply_graph_audio_actions(&mut outcome, connection)?;
        runtime.apply_font_actions(&mut outcome);
        runtime.pending_fetches = std::mem::take(&mut outcome.fetch_actions);
        runtime.pending_websockets = std::mem::take(&mut outcome.websocket_actions);
        runtime.pending_databases = std::mem::take(&mut outcome.database_actions);
        runtime.pending_speech_requests = std::mem::take(&mut outcome.speech_actions);
        runtime.pending_notification_requests = std::mem::take(&mut outcome.notification_actions);
        runtime.pending_protocol_handler_requests =
            std::mem::take(&mut outcome.protocol_handler_actions);
        runtime.pending_permission_requests = std::mem::take(&mut outcome.permission_actions);
        runtime.pending_geolocation_requests = std::mem::take(&mut outcome.geolocation_actions);
        runtime.pending_media_device_requests = std::mem::take(&mut outcome.media_device_actions);
        runtime.pending_sensor_requests = std::mem::take(&mut outcome.sensor_actions);
        runtime.pending_clipboard_requests = std::mem::take(&mut outcome.clipboard_actions);
        runtime.pending_worker_actions = std::mem::take(&mut outcome.worker_actions);
        connection.send_network_state_updates(document, &mut outcome)?;
        runtime.start_pending_survivable_fetches(connection)?;
        connection.send_state_mutations(document, runtime.last_input_sequence, &mut outcome)?;
        let script_time = script_started.elapsed();

        let style_started = Instant::now();
        let mut style = runtime
            .page
            .refresh_resources_for_viewport(runtime.viewport.style_width, runtime.viewport.height);
        runtime.start_presentational_preloads(connection)?;
        let style_time = style_started.elapsed();
        runtime
            .text
            .borrow_mut()
            .register_web_fonts(&runtime.page.fonts);
        let layout_started = Instant::now();
        runtime.rebuild_layout();
        // The initial realm is created before the first layout checkpoint. Publish the actual
        // viewport after that checkpoint just as the resize path does, so scripts that installed
        // responsive layout handlers during startup can replace provisional zero-size geometry.
        let mut viewport_outcome = runtime
            .dispatch_user_input(crate::engine::UserInputEvent::Viewport {
                width: runtime.viewport.style_width,
                height: runtime.viewport.height,
                layout_width: runtime.viewport.width,
                layout_height: runtime.viewport.height,
                scale: runtime.viewport.dpi as f32 / 96.0,
            })?
            .outcome;
        let viewport_render_requested = viewport_outcome.render_requested;
        runtime.admit_user_input_outcome(&mut viewport_outcome, connection)?;
        if viewport_render_requested {
            style = runtime
                .page
                .refresh_resources_after_invalidation_for_viewport(
                    runtime.viewport.style_width,
                    runtime.viewport.height,
                    &viewport_outcome.invalidation,
                );
            runtime.rebuild_layout();
        }
        merge_outcome(
            &mut outcome,
            viewport_outcome,
            runtime.page.dom.document.id(),
        );
        let layout_time = layout_started.elapsed();
        let report = runtime
            .text
            .borrow_mut()
            .finish_load_report(PageLoadReport {
                parse_micros: micros(parse_started.elapsed()),
                html_parse_micros: micros(html_parse_time),
                resource_processing_micros: micros(resource_processing_time),
                script_micros: micros(script_time),
                script_fetch_micros: micros(script_fetch_time),
                style_micros: micros(style_time),
                layout_micros: micros(layout_time),
                ..PageLoadReport::default()
            });
        let presentation = runtime.presentation(outcome, style, report, connection, None)?;
        Ok(LoadResult::Ready(Box::new(runtime), presentation))
    }
}

fn may_speculate_before_parsing(html: &str) -> bool {
    !html
        .as_bytes()
        .windows(b"http-equiv".len())
        .any(|window| window.eq_ignore_ascii_case(b"http-equiv"))
}

#[cfg(test)]
mod csp_preload_tests {
    use super::may_speculate_before_parsing;

    #[test]
    fn meta_pragma_candidates_defer_speculative_requests_until_parser_policy() {
        assert!(!may_speculate_before_parsing(
            "<meta HTTP-EQUIV='Content-Security-Policy' content=\"default-src 'none'\">\
             <script src='https://cdn.test/app.js'></script>"
        ));
        assert!(may_speculate_before_parsing(
            "<script src='https://cdn.test/app.js'></script>"
        ));
    }
}
