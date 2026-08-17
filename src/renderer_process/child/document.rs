//! Renderer-owned document, DOM, JavaScript realm, decoded resources, and layout state.

mod fetch;
mod resources;
mod text;
mod workers;

use self::resources::fetch_script_source;
use self::text::RendererTextSystem;
use self::workers::RendererWorkers;
use super::connection::ChildConnection;
use crate::engine::invalidation::RenderInvalidation;
use crate::engine::{
    Page, PageResource, ScriptFetchAction, ScriptKind, ScriptOutcome, ScriptRuntime,
    ScriptWorkerAction, StyleRefreshStats, layout_page_with_style_viewport,
};
use crate::limits::{MAX_POST_LOAD_TIMER_CALLBACKS, PAGE_RESOURCE_BUDGET};
use crate::renderer_protocol::{
    DocumentId, DocumentStart, PageLoadReport, PresentedImage, PresentedLayout,
    RendererPresentation, RuntimeReport, StyleReport,
};
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub(super) enum LoadResult {
    Ready(Box<DocumentRuntime>, Box<RendererPresentation>),
    Navigate(String),
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
    pending_worker_actions: Vec<ScriptWorkerAction>,
    workers: RendererWorkers,
    executed_async_scripts: HashSet<String>,
    deferred_resources_loaded: bool,
    revision: u64,
    sent_images: HashSet<String>,
}

impl DocumentRuntime {
    pub(super) fn load(
        start: DocumentStart,
        body: Vec<u8>,
        connection: &mut ChildConnection,
    ) -> Result<LoadResult, String> {
        let parse_started = Instant::now();
        let decoded = crate::winhttp::decode_document(
            &body,
            (!start.content_type.is_empty()).then_some(start.content_type.as_str()),
        );
        let html_parse_started = Instant::now();
        let reader = crate::document::parse_html(&decoded.text, &start.url);
        let mut page = Page::parse_scripted(&decoded.text, &start.url);
        page.character_set = decoded.encoding.to_string();
        let html_parse_time = html_parse_started.elapsed();
        if let Some(url) = page.immediate_refresh_url() {
            return Ok(LoadResult::Navigate(url));
        }

        let mut runtime = Self {
            id: start.document,
            status: start.status,
            page,
            reader,
            script_runtime: None,
            viewport: start.viewport,
            text: RendererTextSystem::new(start.viewport.dpi)?,
            layout: Default::default(),
            loaded_resources: HashSet::new(),
            resource_budget: PAGE_RESOURCE_BUDGET,
            pending_fetches: Vec::new(),
            pending_worker_actions: Vec::new(),
            workers: RendererWorkers::new(),
            executed_async_scripts: HashSet::new(),
            deferred_resources_loaded: false,
            revision: 0,
            sent_images: HashSet::new(),
        };

        let resource_started = Instant::now();
        runtime.fetch_resources(connection, |page, resource| {
            page.resource_blocks_first_paint(resource)
        })?;
        let resource_processing_time = resource_started.elapsed();

        let script_started = Instant::now();
        let document = runtime.id;
        let mut loader =
            |url: &str, kind: ScriptKind| fetch_script_source(connection, document, url, kind);
        let (script_runtime, mut outcome) = runtime
            .page
            .start_first_paint_script_runtime_with_loader_and_cookies(
                &mut loader,
                &start.cookie_header,
            );
        runtime.script_runtime = script_runtime;
        runtime.pending_fetches = std::mem::take(&mut outcome.fetch_actions);
        runtime.pending_worker_actions = std::mem::take(&mut outcome.worker_actions);
        let script_time = script_started.elapsed();

        let style_started = Instant::now();
        let style = runtime.page.refresh_resources(runtime.viewport.style_width);
        let style_time = style_started.elapsed();
        runtime.text.register_web_fonts(&runtime.page.fonts);
        let layout_started = Instant::now();
        runtime.rebuild_layout();
        let layout_time = layout_started.elapsed();
        let report = PageLoadReport {
            parse_micros: micros(parse_started.elapsed()),
            html_parse_micros: micros(html_parse_time),
            resource_processing_micros: micros(resource_processing_time),
            script_micros: micros(script_time),
            style_micros: micros(style_time),
            layout_micros: micros(layout_time),
            text_measure_count: runtime.text.measure_calls as u64,
        };
        let presentation = runtime.presentation(outcome, style, report)?;
        Ok(LoadResult::Ready(Box::new(runtime), Box::new(presentation)))
    }

    pub(super) fn id(&self) -> DocumentId {
        self.id
    }

    pub(super) fn next_timer_micros(&mut self) -> Option<u64> {
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
            || !self.pending_fetches.is_empty()
            || !self.pending_worker_actions.is_empty()
            || self.workers.has_work()
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
    ) -> Result<Option<RendererPresentation>, String> {
        let mut outcome = ScriptOutcome::default();
        let mut resources_changed = false;
        let performed_post_load_pass = self.has_post_load_work();
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
        self.execute_pending_async_scripts(connection, &mut outcome)?;
        self.complete_pending_fetches(connection, &mut outcome)?;
        let document_url = self.page.source_url.clone();
        let document_root = self.page.dom.document.id();
        let worker_actions = std::mem::take(&mut self.pending_worker_actions);
        let mut worker_activity = self.workers.drive(
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
            let mut loader = |url: &str, kind| fetch_script_source(connection, document, url, kind);
            let timed = runtime.advance_time_with_loader(
                elapsed,
                max_callbacks.min(MAX_POST_LOAD_TIMER_CALLBACKS as u32) as usize,
                Some(&mut loader),
            );
            merge_outcome(&mut outcome, timed, self.page.dom.document.id());
        }
        self.pending_fetches.append(&mut outcome.fetch_actions);
        let worker_actions = std::mem::take(&mut outcome.worker_actions);
        worker_activity |= self.workers.drive(
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

        let needs_present = resources_changed
            || performed_post_load_pass
            || worker_activity
            || outcome.render_requested
            || outcome.executed > 0
            || !outcome.errors.is_empty()
            || !outcome.console.is_empty()
            || !outcome.diagnostics.is_empty()
            || outcome.navigation_url.is_some()
            || !outcome.cookie_updates.is_empty();
        if !needs_present {
            return Ok(None);
        }
        let style = if outcome.render_requested {
            self.page.refresh_resources_after_invalidation(
                self.viewport.style_width,
                &outcome.invalidation,
            )
        } else {
            StyleRefreshStats::default()
        };
        let layout_started = Instant::now();
        if resources_changed || outcome.render_requested {
            self.rebuild_layout();
        }
        let load = PageLoadReport {
            layout_micros: micros(layout_started.elapsed()),
            text_measure_count: self.text.measure_calls as u64,
            ..PageLoadReport::default()
        };
        self.presentation(outcome, style, load).map(Some)
    }

    pub(super) fn resize(
        &mut self,
        viewport: crate::renderer_protocol::PresentedViewport,
    ) -> Result<RendererPresentation, String> {
        self.viewport = viewport.validate().map_err(|error| error.to_string())?;
        self.text.set_dpi(viewport.dpi);
        let style = self.page.refresh_resources(viewport.style_width);
        let started = Instant::now();
        self.rebuild_layout();
        self.presentation(
            ScriptOutcome::default(),
            style,
            PageLoadReport {
                layout_micros: micros(started.elapsed()),
                text_measure_count: self.text.measure_calls as u64,
                ..PageLoadReport::default()
            },
        )
    }

    fn rebuild_layout(&mut self) {
        self.text.measure_calls = 0;
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
        Ok(RendererPresentation {
            document: self.id,
            revision: self.revision,
            title: self.page.title.clone(),
            final_url: self.page.source_url.clone(),
            status: self.status,
            character_set: self.page.character_set.clone(),
            reader: self.reader.clone(),
            layout: PresentedLayout::from_layout(self.layout.clone()),
            images,
            runtime: runtime_report(outcome, self.script_runtime.is_some()),
            style: style_report(style),
            load,
            next_timer_micros,
        })
    }
}

fn merge_outcome(
    target: &mut ScriptOutcome,
    mut source: ScriptOutcome,
    document_root: crate::engine::dom::NodeId,
) {
    if source.render_requested && source.invalidation.is_empty() {
        source.invalidation = RenderInvalidation::full(document_root);
    }
    target
        .invalidation
        .merge_conservatively(source.invalidation, document_root);
    target.executed = target.executed.saturating_add(source.executed);
    target.mutation_count = target.mutation_count.saturating_add(source.mutation_count);
    target.errors.append(&mut source.errors);
    target.console.append(&mut source.console);
    target.diagnostics.append(&mut source.diagnostics);
    if source.navigation_url.is_some() {
        target.navigation_url = source.navigation_url;
    }
    target.cookie_updates.append(&mut source.cookie_updates);
    target.fetch_actions.append(&mut source.fetch_actions);
    target.worker_actions.append(&mut source.worker_actions);
    target.runtime_stopped |= source.runtime_stopped;
    target.render_requested |= source.render_requested;
}

fn runtime_report(mut outcome: ScriptOutcome, runtime_active: bool) -> RuntimeReport {
    RuntimeReport {
        scripts_executed: outcome.executed as u64,
        dom_mutations: outcome.mutation_count as u64,
        errors: std::mem::take(&mut outcome.errors),
        console: std::mem::take(&mut outcome.console),
        diagnostics: std::mem::take(&mut outcome.diagnostics),
        navigation_url: outcome.navigation_url,
        cookie_updates: outcome.cookie_updates,
        runtime_active,
        runtime_stopped: outcome.runtime_stopped,
        render_requested: outcome.render_requested,
    }
}

fn style_report(style: StyleRefreshStats) -> StyleReport {
    StyleReport {
        invalidated_nodes: style.invalidated_nodes as u64,
        total_styles: style.total_styles as u64,
        recomputed_styles: style.recomputed_styles as u64,
        changed_styles: style.changed_styles as u64,
        removed_styles: style.removed_styles as u64,
        layout_changed: style.layout_changed,
        full_rebuild: style.full_rebuild,
    }
}

fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}
