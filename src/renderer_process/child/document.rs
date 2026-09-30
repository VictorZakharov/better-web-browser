//! Renderer-owned document, DOM, JavaScript realm, decoded resources, and layout state.

mod accessibility;
mod diagnostics;
mod document_streams;
mod dynamic_scripts;
mod fetch;
mod file_picker;
mod font_actions;
mod frames_paint;
mod fullscreen;
mod geometry;
mod graph_audio;
mod image_deltas;
mod interaction;
mod load;
mod media;
mod media_environment;
mod navigation;
mod parser_scripts;
mod parsing;
mod pointer_lock;
mod rendering;
mod reporting;
mod resources;
mod scheduling;
mod text;
mod wake_lock;
mod workers;

use self::accessibility::RendererAccessibility;
use self::dynamic_scripts::{PendingDynamicScriptFetch, advance_dynamic_script_slice};
use self::reporting::{
    merge_outcome, micros, runtime_report, runtime_report_with_wheel, style_report,
};
use self::resources::{PendingResourceFetch, discard_resource_preloads, start_resource_preloads};
pub(super) use self::text::RendererTextSystem;
use self::workers::RendererWorkers;
use super::connection::ChildConnection;
use crate::engine::{
    MediaEnvironment, Page, PageResource, ScriptFetchAction, ScriptKind, ScriptOutcome,
    ScriptRuntime, ScriptWorkerAction, StyleRefreshStats, layout_page_with_style_viewport,
};
use crate::limits::{
    MAX_POST_LOAD_TIMER_CALLBACKS, MAX_RUNTIME_REPORT_ENTRIES, MAX_URL_BYTES, PAGE_RESOURCE_BUDGET,
};
use crate::renderer_protocol::{
    DocumentId, DocumentStart, DocumentState, PageLoadReport, PresentedImage, PresentedLayout,
    RendererPresentation, RendererRuntimeUpdate, WheelAcknowledgement,
};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub(super) enum LoadResult {
    Ready(Box<DocumentRuntime>, AdvanceResult),
    Navigate(String, Box<RendererTextSystem>),
}

pub(super) enum AdvanceResult {
    EncodingRestart,
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
    frame_paint: Vec<frames_paint::PaintedFrame>,
    loaded_resources: HashSet<PageResource>,
    preload_cache: HashMap<resources::preloads::PreloadKey, crate::fetch::FetchResponse>,
    preload_cache_bytes: usize,
    resource_budget: u64,
    pending_fetches: Vec<ScriptFetchAction>,
    pending_websockets: Vec<crate::engine::ScriptWebSocketAction>,
    pending_databases: Vec<crate::engine::ScriptDatabaseAction>,
    pending_speech_requests: Vec<crate::engine::ScriptSpeechAction>,
    pending_notification_requests: Vec<crate::engine::ScriptNotificationAction>,
    pending_protocol_handler_requests: Vec<crate::engine::ScriptProtocolHandlerAction>,
    pending_permission_requests: Vec<crate::renderer_protocol::PermissionRequest>,
    pending_geolocation_requests: Vec<crate::engine::ScriptGeolocationAction>,
    pending_media_device_requests: Vec<crate::engine::ScriptMediaDeviceAction>,
    pending_sensor_requests: Vec<crate::engine::ScriptSensorAction>,
    pending_clipboard_requests: Vec<crate::engine::ScriptClipboardAction>,
    file_pickers: HashMap<u64, crate::renderer_protocol::FileSelectionAssembler>,
    active_script_fetches: HashMap<u64, u32>,
    pending_worker_actions: Vec<ScriptWorkerAction>,
    deferred_network_load: PageLoadReport,
    workers: RendererWorkers,
    parser_scripts: parser_scripts::ParserScripts,
    parser: Option<parsing::DocumentParser>,
    navigation: Option<navigation::StreamingInput>,
    pending_dynamic_script_fetch: Vec<PendingDynamicScriptFetch>,
    pending_resource_preloads: Vec<PendingResourceFetch>,
    resource_render_pending: bool,
    resource_event_pending: bool,
    resource_style_refresh_pending: bool,
    lifecycle: crate::renderer_protocol::DocumentLifecycle,
    accessibility: RendererAccessibility,
    accessibility_selection: Option<(crate::engine::dom::NodeId, u32, u32)>,
    accessibility_values: HashMap<crate::engine::dom::NodeId, String>,
    focused_node: Option<crate::engine::dom::NodeId>,
    pointer_down: [Option<crate::engine::dom::NodeId>; 3],
    scroll_drag: Option<(crate::engine::dom::NodeId, bool, f32, f32)>,
    scriptless_pointer_path: Vec<crate::engine::dom::NodeRef>,
    last_input_sequence: u64,
    native_text_generation: u32,
    last_acknowledged_revision: u64,
    revision: u64,
    sent_images: HashSet<String>,
    last_served_image_key: Option<String>,
    image_budget_warning_sent: bool,
    diagnostic_selectors: Vec<String>,
    prefers_dark_color_scheme: bool,
    media: Option<media::MediaPlayback>,
    media_captions:
        HashMap<crate::engine::dom::NodeId, Vec<crate::engine::script::ScriptCaptionCue>>,
    media_activation: media::MediaActivation,
    media_failure: Option<String>,
    pending_media_action: Option<media::PendingMediaAction>,
    graph_audio_stream: Option<u32>,
    graph_audio_last_stream_id: u32,
    pending_graph_audio_chunk: Option<crate::engine::script::ScriptGraphAudioAction>,
    pending_graph_audio_start: Option<u32>,
    pending_graph_audio_close: Option<(u32, u64)>,
    pending_async_outcome: ScriptOutcome,
    resource_events: resources::events::ResourceEvents,
    geometry_observers_pending: bool,
    resize_observers_pending: bool,
    rendering: rendering::RenderBlocking,
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

    pub(super) fn synchronize_storage(
        &mut self,
        update: crate::storage::StorageUpdate,
    ) -> Result<bool, String> {
        match self.script_runtime.as_mut() {
            Some(runtime) => runtime
                .synchronize_storage(update)
                .map_err(|error| error.to_string()),
            None => Ok(false),
        }
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
    ) -> Result<AdvanceResult, String> {
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
        self.presentation(outcome, style, load, connection, None)
    }

    fn presentation(
        &mut self,
        mut outcome: ScriptOutcome,
        style: StyleRefreshStats,
        load: PageLoadReport,
        connection: &mut ChildConnection,
        wheel: Option<WheelAcknowledgement>,
    ) -> Result<AdvanceResult, String> {
        if self.rendering_is_blocked() {
            return Ok(self.blocked_render_update(outcome, load, wheel));
        }
        self.deliver_geometry_observers(&mut outcome, connection)?;
        self.presentation_after_observers(outcome, style, load, wheel)
    }

    fn presentation_after_observers(
        &mut self,
        mut outcome: ScriptOutcome,
        style: StyleRefreshStats,
        load: PageLoadReport,
        wheel: Option<WheelAcknowledgement>,
    ) -> Result<AdvanceResult, String> {
        if let Some(runtime) = self.script_runtime.as_ref() {
            self.focused_node = runtime.focused_node_id();
        }
        if self.rendering_is_blocked() {
            return Ok(self.blocked_render_update(outcome, load, wheel));
        }
        self.rendering.dirty = false;
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
        let mut retired_image_keys = Vec::new();
        let mut active_canvas_keys = self
            .page
            .images
            .keys()
            .filter(|key| Page::is_canvas_image_key(key))
            .cloned()
            .collect();
        frames_paint::canvas_image_keys(&self.frame_paint, &mut active_canvas_keys);
        self.sent_images.retain(|key| {
            let retired = Page::is_canvas_image_key(key) && !active_canvas_keys.contains(key);
            if retired {
                retired_image_keys.push(key.clone());
            }
            !retired
        });
        let mut seen_candidates = HashSet::new();
        let mut candidates = Vec::new();
        for (url, image) in &self.page.images {
            let canvas_update = self.page.has_canvas_image_update(url);
            let already_sent = self.sent_images.contains(url);
            if url.len() <= MAX_URL_BYTES
                && (!already_sent || canvas_update)
                && seen_candidates.insert(url.clone())
            {
                candidates.push(image_deltas::ImageDelta {
                    presented: PresentedImage {
                        url: url.clone(),
                        image: image.clone(),
                    },
                    canvas_update,
                    frame: None,
                    already_sent,
                });
            }
        }
        frames_paint::append_image_candidates(
            &self.frame_paint,
            &self.sent_images,
            &mut seen_candidates,
            &mut candidates,
        );
        image_deltas::order_for_delivery(&mut candidates, self.last_served_image_key.as_deref());
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
        let mut presentation = RendererPresentation {
            document: self.id,
            revision: self.revision,
            clock_advanced: false,
            title: self.page.title.clone(),
            final_url: self.page.source_url.clone(),
            status: self.status,
            character_set: self.page.character_set.clone(),
            reader: self.reader.clone(),
            layout: PresentedLayout::from_layout(self.layout.clone()),
            images: Vec::new(),
            retired_image_keys,
            glyph_epoch,
            glyphs,
            // Include edge metadata before image capacity is computed from wire size.
            runtime: runtime_report_with_wheel(
                outcome,
                self.script_runtime.is_some(),
                self.media_runtime_report(),
                wheel,
            ),
            style: style_report(style),
            load,
            page_diagnostics,
            accessibility,
            // Some(0) is the largest possible timer encoding. Deferred image
            // work may change the actual timer after the wire-size preflight.
            next_timer_micros: Some(0),
        };
        self.append_bounded_images(&mut presentation, &candidates)?;
        presentation.next_timer_micros = self.next_timer_micros();
        Ok(AdvanceResult::Presentation(Box::new(presentation)))
    }
}
