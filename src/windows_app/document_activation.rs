//! Commits browser-fetched bytes to the page-owning renderer and installs validated output.

use super::browser_navigation::HistoryMode;
use super::*;
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentStart, DocumentState, PresentedViewport, RendererPresentation,
};

pub(super) struct LoadedPage {
    pub(super) body: Vec<u8>,
    pub(super) final_url: String,
    pub(super) status: u16,
    pub(super) content_type: String,
    pub(super) bytes: u64,
    pub(super) network_time: Duration,
}

impl LoadedPage {
    pub(super) fn home() -> Self {
        Self {
            body: HOME_HTML.as_bytes().to_vec(),
            final_url: HOME_URL.into(),
            status: 200,
            content_type: "text/html".into(),
            bytes: HOME_HTML.len() as u64,
            network_time: Duration::ZERO,
        }
    }
}

#[derive(Default)]
pub(super) struct RendererLoadMetrics {
    pub(super) final_url: String,
    pub(super) status: u16,
    pub(super) bytes: u64,
    pub(super) network_time: Duration,
}

pub(super) struct LoadMessage {
    pub generation: u64,
    pub result: Result<LoadedPage, String>,
}

impl BrowserState {
    pub(super) unsafe fn finish_navigation(&mut self, message: LoadMessage) {
        if message.generation != self.generation {
            return;
        }
        match message.result {
            Ok(page) => {
                self.tabs.active_mut().pending_renderer_page = Some(page);
                self.submit_pending_renderer_document();
            }
            Err(error) => {
                self.loading = false;
                self.set_status(&format!("Load failed: {error}"));
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.error = Some(error);
                    benchmark.page_ready = benchmark.process_started.elapsed();
                }
                self.schedule_benchmark_finish();
            }
        }
    }

    pub(super) unsafe fn submit_pending_renderer_document_for(&mut self, id: tabs::TabId) {
        self.process_for_tab(id, |state| state.submit_pending_renderer_document());
    }

    unsafe fn submit_pending_renderer_document(&mut self) {
        let Some(page) = self.pending_renderer_page.take() else {
            return;
        };
        let Some(session) = self.renderer_session.as_ref() else {
            self.pending_renderer_page = Some(page);
            self.start_renderer_for(self.id);
            return;
        };
        let document = match DocumentId::new(self.generation) {
            Ok(document) => document,
            Err(error) => {
                self.loading = false;
                self.set_status(&format!("Renderer document rejected: {error}"));
                return;
            }
        };
        let body_length = match u32::try_from(page.body.len()) {
            Ok(length) => length,
            Err(_) => {
                self.loading = false;
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
        };
        let state = match (
            self.http_client.document_cookie_snapshot(&page.final_url),
            self.local_storage
                .snapshot(&page.final_url)
                .map_err(|error| error.to_string()),
            self.session_storage
                .snapshot(&page.final_url)
                .map_err(|error| error.to_string()),
        ) {
            (Ok(cookie), Ok(local_storage), Ok(session_storage)) => DocumentState {
                cookie_version: cookie.version,
                cookie_header: cookie.header,
                local_storage,
                session_storage,
            },
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                self.loading = false;
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
        match session.load_document(start, state, page.body) {
            Ok(()) => {
                self.reader_url.clone_from(&metrics.final_url);
                self.renderer_document = Some(document);
                self.renderer_input_sequence = 0;
                self.renderer_input_poll_budget = 0;
                self.pending_renderer_inputs.clear();
                self.renderer_revision = 0;
                self.renderer_load_metrics = Some(metrics);
                self.renderer_next_timer = None;
                self.renderer_runtime_clock = Some(Instant::now());
                self.renderer_work_pending = true;
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
                self.loading = false;
                self.contain_page_engine_failure(
                    self.id,
                    format!("could not transfer the document to its renderer: {error}"),
                );
            }
        }
    }

    pub(super) unsafe fn renderer_viewport(&self) -> PresentedViewport {
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let scale = self.page_scale();
        let width = client.right.max(1) as f32 / scale;
        PresentedViewport {
            width,
            height: self.viewport_height().max(1) as f32 / scale,
            style_width: if self.media_viewport_width > 0.0 {
                self.media_viewport_width
            } else {
                width
            },
            dpi: self.dpi,
        }
    }

    pub(super) unsafe fn activate_renderer_presentation(
        &mut self,
        mut presentation: RendererPresentation,
    ) {
        if self.renderer_document != Some(presentation.document)
            || presentation.revision <= self.renderer_revision
        {
            return;
        }
        let first_presentation = self.renderer_revision == 0;
        self.renderer_revision = presentation.revision;

        if let Some(url) = presentation.runtime.navigation_url.as_deref()
            && url != presentation.final_url
            && self.allow_script_navigation(url)
        {
            self.acknowledge_renderer_presentation(
                presentation.document,
                presentation.revision,
                false,
                false,
            );
            self.begin_navigation(url.to_string(), HistoryMode::Script);
            return;
        }

        let accessibility_update = match self.accessibility_document.apply(
            presentation.document,
            presentation.revision,
            presentation.accessibility.clone(),
        ) {
            Ok(update) => update,
            Err(error) => {
                self.contain_page_engine_failure(
                    self.id,
                    format!("renderer accessibility tree was rejected: {error}"),
                );
                return;
            }
        };

        let presentation_install_started = Instant::now();
        let previous_layout = std::mem::take(&mut self.page_layout);
        self.page_layout = std::mem::take(&mut presentation.layout).into_layout();
        self.page_diagnostics = std::mem::take(&mut presentation.page_diagnostics);
        let damage = DisplayListDamage::between(&previous_layout, &self.page_layout);
        let retained_items = self.page_layout.items.clone();
        self.paint_index.rebuild(&retained_items);
        if !self.processing_background_tab {
            self.metrics
                .set_retained_draw_items(self.page_layout.items.len());
        }
        self.content_height = (self.page_layout.content_height * self.page_scale()).ceil() as i32;
        if first_presentation {
            self.presented_images.clear();
        }
        for image in std::mem::take(&mut presentation.images) {
            self.presented_images.insert(image.url, image.image);
        }
        if first_presentation || self.glyph_epoch != presentation.glyph_epoch {
            self.glyph_epoch = presentation.glyph_epoch;
            self.presented_glyphs.clear();
            self.glyph_bitmaps.clear();
        }
        if presentation
            .glyphs
            .iter()
            .any(|glyph| self.presented_glyphs.contains_key(&glyph.id))
        {
            // Resource IDs are immutable within an epoch. A redefinition is contained by
            // dropping every surface derived from the old pixels before accepting the new batch.
            self.glyph_bitmaps.clear();
        }
        for glyph in std::mem::take(&mut presentation.glyphs) {
            self.presented_glyphs.insert(glyph.id, glyph);
        }
        self.image_bitmaps.clear();
        self.reader_url.clone_from(&presentation.final_url);
        self.surface = Surface::Page;
        self.scroll_y = self.scroll_y.min(self.content_height.max(0));
        self.layout_dirty = false;
        self.loading = false;
        self.crashed = false;
        self.renderer_next_timer = presentation.next_timer_micros.map(Duration::from_micros);
        self.renderer_runtime_clock = Some(Instant::now());
        self.renderer_work_pending = false;
        self.renderer_input_poll_budget = 0;

        let history_index = self.history_index;
        if let Some(current) = self.history.get_mut(history_index) {
            current.clone_from(&presentation.final_url);
        }
        self.script_navigation
            .record_committed(&presentation.final_url);
        self.omnibox_text.clone_from(&presentation.final_url);
        if !self.processing_background_tab {
            set_window_text(self.controls.address, &presentation.final_url);
            set_window_text(self.controls.reader, "Reader");
        }
        self.update_active_tab_title(&presentation.title);
        self.update_scrollbar();
        self.recreate_page_controls();

        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.presentation_install_time += presentation_install_started.elapsed();
        }

        self.record_renderer_presentation_metrics(&presentation, damage, first_presentation);
        let error_count = presentation.runtime.errors.len();
        let script_status = if presentation.runtime.scripts_executed == 0 && error_count == 0 {
            String::new()
        } else {
            format!(
                "  •  JS {} / {} mutations / {error_count} errors",
                presentation.runtime.scripts_executed, presentation.runtime.dom_mutations
            )
        };
        self.set_status(&format!(
            "HTTP {}  •  isolated renderer{script_status}",
            presentation.status
        ));

        if !self.processing_background_tab {
            self.refresh_accessibility_document(&accessibility_update);
            let paint_started = Instant::now();
            InvalidateRect(self.window, null(), 0);
            UpdateWindow(self.window);
            if first_presentation && let Some(benchmark) = self.benchmark.as_mut() {
                benchmark.paint_time = paint_started.elapsed();
            }
        }
        self.acknowledge_renderer_presentation(
            presentation.document,
            presentation.revision,
            true,
            true,
        );
        self.schedule_script_runtime_wakeup();
        if first_presentation {
            self.schedule_benchmark_finish();
        }
        self.document = Some(presentation.reader);
    }

    fn record_renderer_presentation_metrics(
        &mut self,
        presentation: &RendererPresentation,
        damage: DisplayListDamage,
        first: bool,
    ) {
        let script_time = Duration::from_micros(presentation.load.script_micros);
        let style_time = Duration::from_micros(presentation.load.style_micros);
        let layout_time = Duration::from_micros(presentation.load.layout_micros);
        self.record_performance_activity(PerformanceActivity::Script, script_time);
        self.record_performance_activity(PerformanceActivity::Style, style_time);
        self.record_performance_activity(PerformanceActivity::Layout, layout_time);
        let load = first.then(|| self.renderer_load_metrics.take()).flatten();
        let reached_page_ready = load.is_some();
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        if let Some(load) = load {
            benchmark.final_url = presentation.final_url.clone();
            benchmark.status = u32::from(load.status);
            benchmark.bytes = load.bytes;
            benchmark.network_time = load.network_time;
            benchmark.parse_time = Duration::from_micros(presentation.load.parse_micros);
            benchmark.html_parse_time = Duration::from_micros(presentation.load.html_parse_micros);
            benchmark.resource_processing_time =
                Duration::from_micros(presentation.load.resource_processing_micros);
            benchmark.page_ready = benchmark.process_started.elapsed();
        }
        benchmark.script_time += script_time;
        benchmark.style_refresh_time += style_time;
        benchmark.layout_time += layout_time;
        benchmark.layout_build_time += layout_time;
        benchmark.layout_tree_time += layout_time;
        benchmark.text_measure_count = benchmark
            .text_measure_count
            .saturating_add(presentation.load.text_measure_count as usize);
        benchmark.text_shape_cache_hits = benchmark
            .text_shape_cache_hits
            .saturating_add(presentation.load.text_shape_cache_hits as usize);
        benchmark.text_shape_cache_misses = benchmark
            .text_shape_cache_misses
            .saturating_add(presentation.load.text_shape_cache_misses as usize);
        benchmark.text_shape_cache_flushes = benchmark
            .text_shape_cache_flushes
            .saturating_add(presentation.load.text_shape_cache_flushes as usize);
        benchmark.text_shape_cache_entries = presentation.load.text_shape_cache_entries as usize;
        benchmark.font_catalog_time += Duration::from_micros(presentation.load.font_catalog_micros);
        benchmark.font_select_time += Duration::from_micros(presentation.load.font_select_micros);
        benchmark.open_type_shape_time +=
            Duration::from_micros(presentation.load.open_type_shape_micros);
        benchmark.glyph_raster_time += Duration::from_micros(presentation.load.glyph_raster_micros);
        benchmark.presentation_encode_time +=
            Duration::from_micros(presentation.load.presentation_encode_micros);
        benchmark.presentation_decode_time +=
            Duration::from_micros(presentation.load.presentation_decode_micros);
        benchmark.script_executed = benchmark
            .script_executed
            .saturating_add(presentation.runtime.scripts_executed as usize);
        if reached_page_ready {
            benchmark.script_executed_at_page_ready = benchmark.script_executed;
        }
        benchmark.script_mutations = benchmark
            .script_mutations
            .saturating_add(presentation.runtime.dom_mutations as usize);
        benchmark
            .script_errors
            .extend(presentation.runtime.errors.iter().cloned());
        benchmark
            .script_console
            .extend(presentation.runtime.console.iter().cloned());
        benchmark
            .script_diagnostics
            .extend(presentation.runtime.diagnostics.iter().cloned());
        benchmark.script_runtime_stopped |= presentation.runtime.runtime_stopped;
        if !first && presentation.runtime.render_requested {
            benchmark.render_checkpoints = benchmark.render_checkpoints.saturating_add(1);
            benchmark.render_mutations = benchmark
                .render_mutations
                .saturating_add(presentation.runtime.dom_mutations as usize);
            benchmark.invalidated_nodes = benchmark
                .invalidated_nodes
                .saturating_add(presentation.style.invalidated_nodes as usize);
            benchmark.style_nodes_recomputed = benchmark
                .style_nodes_recomputed
                .saturating_add(presentation.style.recomputed_styles as usize);
            benchmark.style_nodes_full_rebuild = benchmark
                .style_nodes_full_rebuild
                .saturating_add(presentation.style.total_styles as usize);
            benchmark.full_style_rebuilds = benchmark
                .full_style_rebuilds
                .saturating_add(usize::from(presentation.style.full_rebuild));
            benchmark.full_layout_rebuilds = benchmark.full_layout_rebuilds.saturating_add(1);
            benchmark.display_items_invalidated = benchmark
                .display_items_invalidated
                .saturating_add(damage.changed_items);
            benchmark.full_paint_repaints = benchmark
                .full_paint_repaints
                .saturating_add(usize::from(damage.full_repaint));
        }
    }
}
