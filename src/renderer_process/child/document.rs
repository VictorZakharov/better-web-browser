//! Renderer-owned document, DOM, JavaScript realm, decoded resources, and layout state.

mod accessibility;
mod diagnostics;
mod fetch;
mod interaction;
mod reporting;
mod resources;
mod text;
mod workers;

use self::accessibility::RendererAccessibility;
use self::reporting::{merge_outcome, micros, runtime_report, style_report};
use self::resources::{
    PendingResourceFetch, discard_resource_preloads, fetch_script_source, start_resource_preloads,
};
pub(super) use self::text::RendererTextSystem;
use self::workers::RendererWorkers;
use super::connection::ChildConnection;
use crate::engine::{
    Page, PageResource, ScriptFetchAction, ScriptKind, ScriptOutcome, ScriptRuntime,
    ScriptWorkerAction, StyleRefreshStats, layout_page_with_style_viewport,
};
use crate::limits::{MAX_POST_LOAD_TIMER_CALLBACKS, PAGE_RESOURCE_BUDGET};
use crate::renderer_protocol::{
    DocumentId, DocumentStart, DocumentState, PageLoadReport, PresentedImage, PresentedLayout,
    RendererPresentation, RendererRuntimeUpdate,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub(super) enum LoadResult {
    Ready(Box<DocumentRuntime>, Box<RendererPresentation>),
    Navigate(String, Box<RendererTextSystem>),
}

pub(super) enum AdvanceResult {
    Presentation(Box<RendererPresentation>),
    Runtime(Box<RendererRuntimeUpdate>),
}

pub(super) struct DocumentRuntime {
    id: DocumentId,
    status: u16,
    page: Page,
    reader: crate::document::Document,
    script_runtime: Option<ScriptRuntime>,
    viewport: crate::renderer_protocol::PresentedViewport,
    text: RendererTextSystem,
    layout: crate::engine::LayoutOutput,
    loaded_resources: HashSet<PageResource>,
    resource_budget: u64,
    pending_fetches: Vec<ScriptFetchAction>,
    active_script_fetches: HashMap<u64, u32>,
    pending_worker_actions: Vec<ScriptWorkerAction>,
    deferred_network_load: PageLoadReport,
    workers: RendererWorkers,
    executed_async_scripts: HashSet<String>,
    pending_resource_preloads: Option<PendingResourceFetch>,
    deferred_resources_loaded: bool,
    lifecycle: crate::renderer_protocol::DocumentLifecycle,
    accessibility: RendererAccessibility,
    accessibility_selection: Option<(crate::engine::dom::NodeId, u32, u32)>,
    accessibility_values: HashMap<crate::engine::dom::NodeId, String>,
    focused_node: Option<crate::engine::dom::NodeId>,
    pointer_down: Option<(
        crate::engine::dom::NodeId,
        crate::renderer_protocol::PointerButton,
    )>,
    last_input_sequence: u64,
    last_acknowledged_revision: u64,
    revision: u64,
    sent_images: HashSet<String>,
    diagnostic_selectors: Vec<String>,
}

impl DocumentRuntime {
    pub(super) fn load(
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
            deferred_resources_loaded: false,
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

    pub(super) fn id(&self) -> DocumentId {
        self.id
    }

    pub(super) fn source_url(&self) -> &str {
        &self.page.source_url
    }

    pub(super) fn replace_cookie_snapshot(&mut self, version: u64, header: &str) {
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.replace_cookie_snapshot(version, header);
        }
    }

    pub(super) fn replace_storage_snapshot(
        &mut self,
        area: crate::storage::StorageAreaKind,
        snapshot: crate::storage::StorageAreaSnapshot,
    ) -> Result<(), String> {
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime
                .replace_storage_snapshot(area, snapshot)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(super) fn into_text(mut self) -> RendererTextSystem {
        self.text.reset_for_navigation();
        self.text
    }

    pub(super) fn next_timer_micros(&mut self) -> Option<u64> {
        if self.lifecycle == crate::renderer_protocol::DocumentLifecycle::Frozen {
            return None;
        }
        let runtime_timer = self
            .script_runtime
            .as_mut()
            .and_then(ScriptRuntime::next_timer_delay)
            .map(|delay| delay.as_micros().min(u64::MAX as u128) as u64);
        if self.has_post_load_work() {
            Some(runtime_timer.unwrap_or(0).min(10_000))
        } else {
            runtime_timer
        }
    }

    fn has_post_load_work(&self) -> bool {
        !self.deferred_resources_loaded
            || self.pending_resource_preloads.is_some()
            || !self.pending_fetches.is_empty()
            || !self.pending_worker_actions.is_empty()
            || self.workers.has_work()
            || self
                .script_runtime
                .as_ref()
                .is_some_and(ScriptRuntime::has_pending_dynamic_scripts)
            || self.page.scripts.iter().any(|script| {
                !script.blocks_first_paint
                    && !self.executed_async_scripts.contains(&script.source_url)
            })
    }

    pub(super) fn advance(
        &mut self,
        elapsed: Duration,
        max_callbacks: u32,
        connection: &mut ChildConnection,
    ) -> Result<AdvanceResult, String> {
        if self.lifecycle == crate::renderer_protocol::DocumentLifecycle::Frozen {
            return Ok(AdvanceResult::Runtime(Box::new(RendererRuntimeUpdate {
                document: self.id,
                clock_advanced: true,
                runtime: runtime_report(ScriptOutcome::default(), self.script_runtime.is_some()),
                load: PageLoadReport::default(),
                next_timer_micros: None,
            })));
        }
        let mut outcome = ScriptOutcome::default();
        let mut resources_changed = false;
        let mut script_time = Duration::ZERO;
        if let Some(pending) = self.pending_resource_preloads.take() {
            resources_changed |= self.finish_resource_preloads(connection, pending)?;
        }
        if !self.deferred_resources_loaded {
            resources_changed = self.fetch_resources(connection, |_, resource| {
                matches!(
                    resource,
                    PageResource::Image { .. } | PageResource::Font { .. }
                )
            })?;
            self.deferred_resources_loaded = true;
            if resources_changed {
                self.text.register_web_fonts(&self.page.fonts);
            }
        }
        let async_script_started = Instant::now();
        self.execute_pending_async_scripts(connection, &mut outcome)?;
        script_time += async_script_started.elapsed();
        self.start_pending_fetches(connection)?;
        let document_url = self.page.source_url.clone();
        let document_root = self.page.dom.document.id();
        let worker_actions = std::mem::take(&mut self.pending_worker_actions);
        self.workers.drive(
            worker_actions,
            workers::WorkerDriveContext {
                connection,
                document: self.id,
                document_url: &document_url,
                runtime: &mut self.script_runtime,
                document_root,
                outcome: &mut outcome,
            },
        )?;

        if let Some(runtime) = self.script_runtime.as_mut() {
            let document = self.id;
            let mut loader = |url: &str, kind, options| {
                fetch_script_source(connection, document, url, kind, options)
            };
            let timer_started = Instant::now();
            let timed = runtime.advance_time_with_loader(
                elapsed,
                max_callbacks.min(MAX_POST_LOAD_TIMER_CALLBACKS as u32) as usize,
                Some(&mut loader),
            );
            script_time += timer_started.elapsed();
            merge_outcome(&mut outcome, timed, self.page.dom.document.id());
        }
        self.pending_fetches.append(&mut outcome.fetch_actions);
        let worker_actions = std::mem::take(&mut outcome.worker_actions);
        self.workers.drive(
            worker_actions,
            workers::WorkerDriveContext {
                connection,
                document: self.id,
                document_url: &document_url,
                runtime: &mut self.script_runtime,
                document_root,
                outcome: &mut outcome,
            },
        )?;
        self.pending_fetches.append(&mut outcome.fetch_actions);
        connection.send_state_mutations(self.id, &mut outcome)?;

        // Script execution, console output, storage/cookie traffic, and worker progress are not
        // visual invalidations. Sending a complete display-list snapshot for those tasks made
        // timer-heavy pages continuously serialize, install, and repaint an unchanged document.
        let needs_present = resources_changed || outcome.render_requested;
        let style = if outcome.render_requested {
            self.page.refresh_resources_after_invalidation_for_viewport(
                self.viewport.style_width,
                self.viewport.height,
                &outcome.invalidation,
            )
        } else {
            StyleRefreshStats::default()
        };
        let layout_started = Instant::now();
        if resources_changed || outcome.render_requested {
            self.rebuild_layout();
        }
        let load = self.text.finish_load_report(PageLoadReport {
            script_micros: micros(script_time),
            layout_micros: micros(layout_started.elapsed()),
            ..PageLoadReport::default()
        });
        if needs_present {
            self.presentation(outcome, style, load)
                .map(|mut presentation| {
                    presentation.clock_advanced = true;
                    AdvanceResult::Presentation(Box::new(presentation))
                })
        } else {
            let next_timer_micros = self.next_timer_micros();
            Ok(AdvanceResult::Runtime(Box::new(RendererRuntimeUpdate {
                document: self.id,
                clock_advanced: true,
                runtime: runtime_report(outcome, self.script_runtime.is_some()),
                load,
                next_timer_micros,
            })))
        }
    }

    pub(super) fn resize(
        &mut self,
        viewport: crate::renderer_protocol::PresentedViewport,
        connection: &mut ChildConnection,
    ) -> Result<RendererPresentation, String> {
        self.viewport = viewport.validate().map_err(|error| error.to_string())?;
        self.text.set_dpi(viewport.dpi);
        let mut outcome = self
            .dispatch_user_input(crate::engine::UserInputEvent::Viewport {
                width: viewport.width,
                height: viewport.height,
                scale: viewport.dpi as f32 / 96.0,
            })?
            .outcome;
        self.admit_user_input_outcome(&mut outcome, connection)?;
        let style = self
            .page
            .refresh_resources_for_viewport(viewport.style_width, viewport.height);
        let started = Instant::now();
        self.rebuild_layout();
        let load = self.text.finish_load_report(PageLoadReport {
            layout_micros: micros(started.elapsed()),
            ..PageLoadReport::default()
        });
        self.presentation(outcome, style, load)
    }

    fn rebuild_layout(&mut self) {
        self.text.reset_layout_metrics();
        self.layout = layout_page_with_style_viewport(
            &self.page,
            self.viewport.width,
            self.viewport.height,
            self.viewport.style_width,
            &mut self.text,
        );
    }

    fn presentation(
        &mut self,
        outcome: ScriptOutcome,
        style: StyleRefreshStats,
        load: PageLoadReport,
    ) -> Result<RendererPresentation, String> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| "presentation revision exhausted".to_string())?;
        let mut images = Vec::new();
        for (url, image) in &self.page.images {
            if self.sent_images.insert(url.clone()) {
                images.push(PresentedImage {
                    url: url.clone(),
                    image: image.clone(),
                });
            }
        }
        let next_timer_micros = self.next_timer_micros();
        let glyph_epoch = self.text.glyph_epoch();
        let glyphs = self.text.take_pending_glyphs();
        let page_diagnostics = diagnostics::collect(
            &self.page,
            &self.layout,
            &self.diagnostic_selectors,
            self.viewport.style_width,
            self.viewport.height,
        );
        let accessibility = self.accessibility.update(
            &self.page,
            &self.layout,
            self.viewport,
            self.focused_node,
            self.accessibility_selection,
            &self.accessibility_values,
        )?;
        Ok(RendererPresentation {
            document: self.id,
            revision: self.revision,
            clock_advanced: false,
            title: self.page.title.clone(),
            final_url: self.page.source_url.clone(),
            status: self.status,
            character_set: self.page.character_set.clone(),
            reader: self.reader.clone(),
            layout: PresentedLayout::from_layout(self.layout.clone()),
            images,
            glyph_epoch,
            glyphs,
            runtime: runtime_report(outcome, self.script_runtime.is_some()),
            style: style_report(style),
            load,
            page_diagnostics,
            accessibility,
            next_timer_micros,
        })
    }
}
