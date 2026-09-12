//! Initial document decoding, speculative loading, script startup, and first presentation.

use super::*;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn load(
        start: DocumentStart,
        state: DocumentState,
        body: Vec<u8>,
        connection: &mut ChildConnection,
        mut text: RendererTextSystem,
    ) -> Result<LoadResult, String> {
        let parse_started = Instant::now();
        let decoded = crate::winhttp::decode_document(
            &body,
            (!start.content_type.is_empty()).then_some(start.content_type.as_str()),
        );
        // The HTML Standard permits speculative parsing to start eligible fetches while the
        // authoritative parser continues. Bytes are cached, but execution is admitted only when
        // that parser reaches the actual element. A speculative response cannot prepare a script:
        // https://html.spec.whatwg.org/multipage/parsing.html#speculative-html-parsing
        let preloads = crate::engine::page::discover_script_preloads(&decoded.text, &start.url);
        let (pending_first_paint, pending_deferred) = start_resource_preloads(
            connection,
            start.document,
            preloads.first_paint,
            preloads.deferred,
        )?;
        let html_parse_started = Instant::now();
        let reader = crate::document::parse_html(&decoded.text, &start.url);
        let mut parser = crate::engine::dom::incremental::HtmlParser::new(&decoded.text);
        let first_step = parser.advance();
        let parser_active = matches!(
            first_step,
            crate::engine::dom::incremental::ParserStep::Script(_)
        );
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
            loaded_resources: HashSet::new(),
            resource_budget: PAGE_RESOURCE_BUDGET,
            pending_fetches: Vec::new(),
            active_script_fetches: HashMap::new(),
            pending_worker_actions: Vec::new(),
            deferred_network_load: PageLoadReport::default(),
            workers: RendererWorkers::new(),
            parser_scripts,
            parser: parser_active.then_some(parsing::DocumentParser {
                parser,
                next: Some(first_step),
            }),
            pending_dynamic_script_fetch: Vec::new(),
            pending_resource_preloads: pending_deferred.into_iter().collect(),
            resource_render_pending: false,
            resource_style_refresh_pending: false,
            lifecycle: crate::renderer_protocol::DocumentLifecycle::Active,
            accessibility: RendererAccessibility::default(),
            accessibility_selection: None,
            accessibility_values: HashMap::new(),
            focused_node: None,
            pointer_down: None,
            scriptless_pointer_path: Vec::new(),
            last_input_sequence: 0,
            last_acknowledged_revision: 0,
            revision: 0,
            sent_images: HashSet::new(),
            diagnostic_selectors: start.diagnostic_selectors,
            prefers_dark_color_scheme: start.prefers_dark_color_scheme,
            media: None,
            media_activation: Default::default(),
            media_failure: None,
            pending_media_action: None,
            pending_async_outcome: ScriptOutcome::default(),
            resource_events: Default::default(),
            geometry_observers_pending: false,
        };

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
            let (script_runtime, initial) = runtime
                .page
                .start_parser_runtime(
                    state.cookie_version,
                    &state.cookie_header,
                    state.local_storage,
                    state.session_storage,
                    !runtime.diagnostic_selectors.is_empty(),
                    runtime.script_layout_flush_callback(),
                )
                .map_err(|error| error.to_string())?;
            runtime.script_runtime = Some(script_runtime);
            outcome = initial;
            runtime.advance_parser(connection, &mut outcome)?;
        }
        runtime.start_dynamic_script_fetches(connection)?;
        runtime.flush_pending_resource_events()?;
        merge_outcome(
            &mut outcome,
            std::mem::take(&mut runtime.pending_async_outcome),
            runtime.page.dom.document.id(),
        );
        runtime.apply_media_actions(&mut outcome, connection)?;
        runtime.pending_fetches = std::mem::take(&mut outcome.fetch_actions);
        runtime.pending_worker_actions = std::mem::take(&mut outcome.worker_actions);
        connection.send_state_mutations(document, &mut outcome)?;
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
        let presentation = runtime.presentation(outcome, style, report)?;
        Ok(LoadResult::Ready(Box::new(runtime), Box::new(presentation)))
    }
}
