//! Renderer-owned document, DOM, JavaScript realm, decoded resources, and layout state.

mod accessibility;
mod diagnostics;
mod dynamic_scripts;
mod fetch;
mod fullscreen;
mod geometry;
mod interaction;
mod load;
mod media;
mod media_environment;
mod parser_scripts;
mod parsing;
mod reporting;
mod resources;
mod scheduling;
mod text;
mod workers;

use self::accessibility::RendererAccessibility;
use self::dynamic_scripts::{PendingDynamicScriptFetch, advance_dynamic_script_slice};
use self::reporting::{merge_outcome, micros, runtime_report, style_report};
use self::resources::{PendingResourceFetch, discard_resource_preloads, start_resource_preloads};
pub(super) use self::text::RendererTextSystem;
use self::workers::RendererWorkers;
use super::connection::ChildConnection;
use crate::engine::{
    MediaEnvironment, Page, PageResource, ScriptFetchAction, ScriptKind, ScriptOutcome,
    ScriptRuntime, ScriptWorkerAction, StyleRefreshStats, layout_page_with_style_viewport,
};
use crate::limits::{
    MAX_POST_LOAD_TIMER_CALLBACKS, MAX_RUNTIME_REPORT_ENTRIES, PAGE_RESOURCE_BUDGET,
};
use crate::renderer_protocol::{
    DocumentId, DocumentStart, DocumentState, PageLoadReport, PresentedImage, PresentedLayout,
    RendererPresentation, RendererRuntimeUpdate,
};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
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
    text: Rc<RefCell<RendererTextSystem>>,
    script_layout_page: Rc<RefCell<Page>>,
    script_layout_viewport: Rc<Cell<crate::renderer_protocol::PresentedViewport>>,
    layout: crate::engine::LayoutOutput,
    loaded_resources: HashSet<PageResource>,
    resource_budget: u64,
    pending_fetches: Vec<ScriptFetchAction>,
    active_script_fetches: HashMap<u64, u32>,
    pending_worker_actions: Vec<ScriptWorkerAction>,
    deferred_network_load: PageLoadReport,
    workers: RendererWorkers,
    parser_scripts: parser_scripts::ParserScripts,
    parser: Option<parsing::DocumentParser>,
    pending_dynamic_script_fetch: Vec<PendingDynamicScriptFetch>,
    pending_resource_preloads: Vec<PendingResourceFetch>,
    resource_render_pending: bool,
    resource_style_refresh_pending: bool,
    lifecycle: crate::renderer_protocol::DocumentLifecycle,
    accessibility: RendererAccessibility,
    accessibility_selection: Option<(crate::engine::dom::NodeId, u32, u32)>,
    accessibility_values: HashMap<crate::engine::dom::NodeId, String>,
    focused_node: Option<crate::engine::dom::NodeId>,
    pointer_down: [Option<crate::engine::dom::NodeId>; 3],
    scriptless_pointer_path: Vec<crate::engine::dom::NodeRef>,
    last_input_sequence: u64,
    last_acknowledged_revision: u64,
    revision: u64,
    sent_images: HashSet<String>,
    diagnostic_selectors: Vec<String>,
    prefers_dark_color_scheme: bool,
    media: Option<media::MediaPlayback>,
    media_activation: media::MediaActivation,
    media_failure: Option<String>,
    pending_media_action: Option<media::PendingMediaAction>,
    pending_async_outcome: ScriptOutcome,
    resource_events: resources::events::ResourceEvents,
    geometry_observers_pending: bool,
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
        self.script_runtime.take();
        let mut text = match Rc::try_unwrap(self.text) {
            Ok(text) => text.into_inner(),
            Err(_) => unreachable!("script layout callback outlived its runtime"),
        };
        text.reset_for_navigation();
        text
    }

    pub(super) fn resize(
        &mut self,
        viewport: crate::renderer_protocol::PresentedViewport,
        connection: &mut ChildConnection,
    ) -> Result<RendererPresentation, String> {
        self.viewport = viewport.validate().map_err(|error| error.to_string())?;
        self.apply_media_environment(viewport);
        self.text.borrow_mut().set_dpi(viewport.dpi);
        self.sync_script_layout_page();
        let mut outcome = self
            .dispatch_user_input(crate::engine::UserInputEvent::Viewport {
                width: viewport.style_width,
                height: viewport.height,
                layout_width: viewport.width,
                layout_height: viewport.height,
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
        let load = self.text.borrow_mut().finish_load_report(PageLoadReport {
            layout_micros: micros(started.elapsed()),
            ..PageLoadReport::default()
        });
        self.presentation(outcome, style, load)
    }

    fn presentation(
        &mut self,
        mut outcome: ScriptOutcome,
        style: StyleRefreshStats,
        load: PageLoadReport,
    ) -> Result<RendererPresentation, String> {
        self.page.title = self.page.dom.title();
        if !self.diagnostic_selectors.is_empty()
            && outcome.diagnostics.len() < MAX_RUNTIME_REPORT_ENTRIES
        {
            let decoded_image_bytes = self.page.images.values().fold(0_usize, |total, image| {
                total.saturating_add(image.bgra.len())
            });
            outcome.diagnostics.push(format!(
                "page resources: {} discovered, {} settled, {} stylesheets, {} decoded images / {} bytes, {} fonts, {} bytes remaining",
                self.page.resources.len(),
                self.loaded_resources.len(),
                self.page.external_stylesheets.len(),
                self.page.images.len(),
                decoded_image_bytes,
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
        let glyph_epoch = self.text.borrow().glyph_epoch();
        let glyphs = self.text.borrow_mut().take_pending_glyphs();
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
            runtime: runtime_report(
                outcome,
                self.script_runtime.is_some(),
                self.media_runtime_report(),
            ),
            style: style_report(style),
            load,
            page_diagnostics,
            accessibility,
            next_timer_micros,
        })
    }
}
