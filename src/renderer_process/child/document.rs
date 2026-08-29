//! Renderer-owned document, DOM, JavaScript realm, decoded resources, and layout state.

mod accessibility;
mod diagnostics;
mod dynamic_scripts;
mod fetch;
mod interaction;
mod load;
mod reporting;
mod resources;
mod text;
mod workers;

use self::accessibility::RendererAccessibility;
use self::dynamic_scripts::{
    PendingDynamicScriptFetch, advance_dynamic_script_slice, start_dynamic_script_preloads,
};
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
use crate::limits::{
    MAX_POST_LOAD_TIMER_CALLBACKS, MAX_RUNTIME_REPORT_ENTRIES, PAGE_RESOURCE_BUDGET,
};
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
    pending_dynamic_script_fetch: Option<PendingDynamicScriptFetch>,
    pending_resource_preloads: Option<PendingResourceFetch>,
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
    prefers_dark_color_scheme: bool,
}

impl DocumentRuntime {
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
        !self.pending_fetches.is_empty()
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
        let mut script_fetch_time = Duration::ZERO;
        let resources_changed = self
            .finish_ready_resource_preloads(connection)?
            .unwrap_or(false);
        if resources_changed {
            self.text.register_web_fonts(&self.page.fonts);
        }
        let mut script_time = Duration::ZERO;
        let async_script_started = Instant::now();
        connection.report_renderer_task_stage(format!(
            "checking deferred scripts for {}",
            self.page.source_url
        ))?;
        self.execute_pending_async_scripts(connection, &mut outcome, &mut script_fetch_time)?;
        script_time += async_script_started.elapsed();
        self.start_pending_fetches(connection)?;
        let document_url = self.page.source_url.clone();
        let document_root = self.page.dom.document.id();
        let worker_actions = std::mem::take(&mut self.pending_worker_actions);
        connection
            .report_renderer_task_stage(format!("driving workers for {}", self.page.source_url))?;
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

        if self.pending_dynamic_script_fetch.is_none()
            && let Some(runtime) = self.script_runtime.as_ref()
        {
            // Dynamic script elements are force-async. Start their network work together, while
            // retaining one script execution per event-loop task below:
            // https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element
            self.pending_dynamic_script_fetch = start_dynamic_script_preloads(
                connection,
                self.id,
                runtime.pending_dynamic_script_requests(),
            )?;
        }

        if let Some(runtime) = self.script_runtime.as_mut() {
            let document = self.id;
            connection.report_renderer_task_stage(format!(
                "settling timers and promise jobs for {}",
                self.page.source_url
            ))?;
            let timer_started = Instant::now();
            let callback_limit = max_callbacks.min(MAX_POST_LOAD_TIMER_CALLBACKS as u32) as usize;
            let timed = if runtime.has_pending_dynamic_scripts() {
                advance_dynamic_script_slice(
                    runtime,
                    &mut self.pending_dynamic_script_fetch,
                    connection,
                    document,
                    document_root,
                    elapsed,
                    callback_limit,
                    &mut script_fetch_time,
                )
            } else {
                let document_url = self.page.source_url.clone();
                let mut reporter = |stage: &str| {
                    let _ = connection
                        .report_renderer_task_stage(format!("{stage} for {document_url}"));
                };
                runtime.advance_time_with_loader_and_stage_reporter(
                    elapsed,
                    callback_limit,
                    None,
                    Some(&mut reporter),
                )
            };
            script_time += timer_started.elapsed();
            merge_outcome(&mut outcome, timed, self.page.dom.document.id());
        }
        self.pending_fetches.append(&mut outcome.fetch_actions);
        let worker_actions = std::mem::take(&mut outcome.worker_actions);
        connection.report_renderer_task_stage(format!(
            "driving post-script workers for {}",
            self.page.source_url
        ))?;
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
            connection.report_renderer_task_stage(format!(
                "refreshing styles for {}",
                self.page.source_url
            ))?;
            self.page.refresh_resources_after_invalidation_for_viewport(
                self.viewport.style_width,
                self.viewport.height,
                &outcome.invalidation,
            )
        } else if resources_changed {
            self.page
                .refresh_resources_for_viewport(self.viewport.style_width, self.viewport.height)
        } else {
            StyleRefreshStats::default()
        };
        // A rendering checkpoint can discover resources in newly-created shadow trees. Start the
        // browser fetch now, but do not wait inside this renderer task. The response path installs
        // the completed batch and presents the resulting layout without blocking heartbeats.
        self.start_presentational_preloads(connection)?;
        let layout_started = Instant::now();
        if resources_changed || outcome.render_requested {
            connection.report_renderer_task_stage(format!(
                "rebuilding layout for {}",
                self.page.source_url
            ))?;
            self.rebuild_layout();
        }
        let load = self.text.finish_load_report(PageLoadReport {
            script_micros: micros(script_time),
            script_fetch_micros: micros(script_fetch_time),
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
        self.page
            .set_media_environment(viewport.style_width, self.prefers_dark_color_scheme);
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.set_media_environment(viewport.style_width, self.prefers_dark_color_scheme);
        }
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
        self.start_presentational_preloads(connection)?;
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
        mut outcome: ScriptOutcome,
        style: StyleRefreshStats,
        load: PageLoadReport,
    ) -> Result<RendererPresentation, String> {
        if !self.diagnostic_selectors.is_empty()
            && outcome.diagnostics.len() < MAX_RUNTIME_REPORT_ENTRIES
        {
            outcome.diagnostics.push(format!(
                "page resources: {} discovered, {} settled, {} stylesheets, {} decoded images, {} fonts, {} bytes remaining",
                self.page.resources.len(),
                self.loaded_resources.len(),
                self.page.external_stylesheets.len(),
                self.page.images.len(),
                self.page.fonts.len(),
                self.resource_budget
            ));
        }
        let remaining = MAX_RUNTIME_REPORT_ENTRIES.saturating_sub(outcome.diagnostics.len());
        outcome
            .diagnostics
            .extend(self.page.diagnostics.drain(..).take(remaining));
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
