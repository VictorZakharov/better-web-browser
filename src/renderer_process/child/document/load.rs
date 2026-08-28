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
        // authoritative parser continues. Results are reconciled with the completed page below,
        // so this optimization cannot introduce scripts the real parse did not discover:
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
        let mut page = Page::parse_scripted(&decoded.text, &start.url);
        page.character_set = decoded.encoding.to_string();
        page.set_media_environment(start.viewport.style_width, start.prefers_dark_color_scheme);
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
        let mut runtime = Self {
            id: start.document,
            status: start.status,
            page,
            reader,
            script_runtime: None,
            viewport: start.viewport,
            text,
            layout: Default::default(),
            loaded_resources: HashSet::new(),
            resource_budget: PAGE_RESOURCE_BUDGET,
            pending_fetches: Vec::new(),
            active_script_fetches: HashMap::new(),
            pending_worker_actions: Vec::new(),
            deferred_network_load: PageLoadReport::default(),
            workers: RendererWorkers::new(),
            executed_async_scripts: HashSet::new(),
            pending_resource_preloads: pending_deferred,
            lifecycle: crate::renderer_protocol::DocumentLifecycle::Active,
            accessibility: RendererAccessibility::default(),
            accessibility_selection: None,
            accessibility_values: HashMap::new(),
            focused_node: None,
            pointer_down: None,
            last_input_sequence: 0,
            last_acknowledged_revision: 0,
            revision: 0,
            sent_images: HashSet::new(),
            diagnostic_selectors: start.diagnostic_selectors,
            prefers_dark_color_scheme: start.prefers_dark_color_scheme,
        };

        let resource_started = Instant::now();
        if let Some(pending) = pending_first_paint {
            runtime.finish_resource_preloads(connection, pending)?;
        }
        runtime.fetch_resources(connection, |page, resource| {
            page.resource_blocks_first_paint(resource)
        })?;
        let resource_processing_time = resource_started.elapsed();

        let script_started = Instant::now();
        let document = runtime.id;
        let mut loader = |url: &str, kind: ScriptKind, options| {
            fetch_script_source(connection, document, url, kind, options)
        };
        let (mut script_runtime, mut outcome) = runtime
            .page
            .start_first_paint_script_runtime_with_document_state(
                &mut loader,
                state.cookie_version,
                &state.cookie_header,
                state.local_storage,
                state.session_storage,
            )
            .map_err(|error| error.to_string())?;
        if let Some(script_runtime) = script_runtime.as_mut() {
            script_runtime.set_host_call_profiling(!runtime.diagnostic_selectors.is_empty());
        }
        connection.send_state_mutations(document, &mut outcome)?;
        runtime.script_runtime = script_runtime;
        runtime.pending_fetches = std::mem::take(&mut outcome.fetch_actions);
        runtime.pending_worker_actions = std::mem::take(&mut outcome.worker_actions);
        let script_time = script_started.elapsed();

        let style_started = Instant::now();
        let style = runtime
            .page
            .refresh_resources_for_viewport(runtime.viewport.style_width, runtime.viewport.height);
        runtime.start_presentational_preloads(connection)?;
        let style_time = style_started.elapsed();
        runtime.text.register_web_fonts(&runtime.page.fonts);
        let layout_started = Instant::now();
        runtime.rebuild_layout();
        let layout_time = layout_started.elapsed();
        let report = runtime.text.finish_load_report(PageLoadReport {
            parse_micros: micros(parse_started.elapsed()),
            html_parse_micros: micros(html_parse_time),
            resource_processing_micros: micros(resource_processing_time),
            script_micros: micros(script_time),
            style_micros: micros(style_time),
            layout_micros: micros(layout_time),
            ..PageLoadReport::default()
        });
        let presentation = runtime.presentation(outcome, style, report)?;
        Ok(LoadResult::Ready(Box::new(runtime), Box::new(presentation)))
    }
}
