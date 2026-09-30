//! Commits browser-fetched bytes to the page-owning renderer and installs validated output.

mod metrics;
mod painting;
mod submission;
mod video;

use super::paint_primitives::screen_rect;
use super::*;
use better_web_browser::renderer_protocol::{
    DocumentStart, DocumentState, PresentedViewport, RendererPresentation,
};

#[derive(Clone)]
pub(super) struct LoadedPage {
    pub(super) stream: Option<better_web_browser::renderer_process::NavigationBody>,
    pub(super) body: Vec<u8>,
    pub(super) final_url: String,
    pub(super) redirected: bool,
    pub(super) status: u16,
    pub(super) content_type: String,
    pub(super) policy: std::sync::Arc<better_web_browser::fetch::csp::PolicyContainer>,
    pub(super) screen_wake_lock_allowed: bool,
    pub(super) bytes: u64,
    pub(super) network_time: Duration,
}

impl LoadedPage {
    pub(super) fn home() -> Self {
        Self {
            stream: None,
            body: HOME_HTML.as_bytes().to_vec(),
            final_url: HOME_URL.into(),
            redirected: false,
            status: 200,
            content_type: "text/html".into(),
            policy: Default::default(),
            screen_wake_lock_allowed: true,
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
    pub result: Result<NavigationResult, String>,
}

pub(super) enum NavigationResult {
    Headers(LoadedPage),
    Complete { bytes: u64, network_time: Duration },
}

impl BrowserState {
    pub(super) unsafe fn finish_navigation(&mut self, message: LoadMessage) {
        match message.result {
            Ok(NavigationResult::Headers(page)) => {
                let completed = Self::network_incident(&page);
                if !self.navigation.accept_page(message.generation, page) {
                    return;
                }
                self.incidents.record("navigation", completed);
                self.submit_pending_renderer_document();
            }
            Ok(NavigationResult::Complete {
                bytes,
                network_time,
            }) => {
                if message.generation != self.navigation.generation() {
                    return;
                }
                self.navigation.network_complete(bytes, network_time);
                if let Some(metrics) = self.renderer_load_metrics.as_mut() {
                    metrics.bytes = bytes;
                    metrics.network_time = network_time;
                }
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.network_time = network_time;
                    benchmark.bytes = bytes;
                }
                self.incidents.record(
                    "navigation",
                    format!(
                        "network complete: {bytes} bytes, {:.1} ms",
                        network_time.as_secs_f64() * 1000.0
                    ),
                );
            }
            Err(error) => {
                if message.generation != self.navigation.generation() {
                    return;
                }
                if self.navigation.active_document().is_some() {
                    // The broker carries the same failure to the renderer. Retire the UI's
                    // active transaction as well instead of leaving a partial page interactive.
                    self.document_fetch.abort();
                    self.contain_page_engine_failure(
                        self.id,
                        format!("navigation response failed: {error}"),
                    );
                    return;
                }
                self.navigation.fail();
                self.incidents
                    .record("navigation", format!("load failed: {error}"));
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
            prefers_dark_color_scheme: self.app.prefers_dark_color_scheme.get(),
        }
    }

    pub(super) unsafe fn activate_renderer_presentation(
        &mut self,
        mut presentation: RendererPresentation,
    ) {
        if !self.navigation.owns_document(presentation.document)
            || presentation.revision <= self.renderer_revision
        {
            return;
        }
        let first_presentation = self.renderer_revision == 0;
        let received = Instant::now();
        if first_presentation {
            // History actions emitted by parser scripts belong to the new document.
            // The shell may still hold the outgoing document's scroll offset here.
            self.scroll_y = 0;
        }
        let presentation_install_started = Instant::now();
        self.renderer_revision = presentation.revision;
        self.record_renderer_presentation_incident(&presentation, first_presentation);

        self.apply_same_document_history_updates(
            presentation.document,
            &presentation.runtime.history_actions,
        );
        if self.follow_runtime_navigation(
            &presentation.runtime,
            Some((presentation.document, presentation.revision)),
        ) {
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

        let mut next_layout = std::mem::take(&mut presentation.layout).into_layout();
        next_layout.update_scroll_position(
            0.0,
            self.scroll_y.max(0) as f32 / self.page_scale().max(f32::EPSILON),
        );
        let damage = DisplayListDamage::between(&self.page_layout, &next_layout);
        let layout_changed = !damage.is_empty();
        let controls_changed = first_presentation
            || self.page_layout.forms != next_layout.forms
            || page_controls::native_controls_changed(&self.page_layout, &next_layout);
        if layout_changed {
            self.page_layout = next_layout;
            let retained_items = self.page_layout.items.clone();
            self.paint_index.rebuild(&retained_items);
            if !self.processing_background_tab {
                self.metrics
                    .set_retained_draw_items(self.page_layout.items.len());
            }
            self.content_height =
                (self.page_layout.content_height * self.page_scale()).ceil() as i32;
        } else {
            self.page_layout.sticky_layers = next_layout.sticky_layers;
        }
        self.sync_retained_control_rects();
        self.page_diagnostics = std::mem::take(&mut presentation.page_diagnostics);
        if first_presentation {
            self.presented_images.clear();
            self.image_bitmaps.clear();
        }
        let images_changed =
            !presentation.images.is_empty() || !presentation.retired_image_keys.is_empty();
        for key in std::mem::take(&mut presentation.retired_image_keys) {
            self.image_bitmaps.remove(&key);
            self.presented_images.remove(&key);
        }
        for image in std::mem::take(&mut presentation.images) {
            if !first_presentation {
                self.image_bitmaps.remove(&image.url);
            }
            self.presented_images.insert(image.url, image.image);
        }
        let glyph_epoch_changed =
            first_presentation || self.glyph_epoch != presentation.glyph_epoch;
        if glyph_epoch_changed {
            self.glyph_epoch = presentation.glyph_epoch;
            self.presented_glyphs.clear();
            self.glyph_bitmaps.clear();
        }
        let glyphs_changed = !presentation.glyphs.is_empty();
        let glyphs_redefined = presentation
            .glyphs
            .iter()
            .any(|glyph| self.presented_glyphs.contains_key(&glyph.id));
        if glyphs_redefined {
            // Resource IDs are immutable within an epoch. A redefinition is contained by
            // dropping every surface derived from the old pixels before accepting the new batch.
            self.glyph_bitmaps.clear();
        }
        for glyph in std::mem::take(&mut presentation.glyphs) {
            self.presented_glyphs.insert(glyph.id, glyph);
        }
        self.reader_url = self
            .current_url()
            .unwrap_or(&presentation.final_url)
            .to_owned();
        self.surface = Surface::Page;
        if layout_changed {
            self.scroll_y = self.scroll_y.min(self.content_height.max(0));
        }
        self.layout_dirty = false;
        self.navigation.mark_presented(presentation.document);
        self.crashed = false;
        self.renderer_next_timer = presentation.next_timer_micros.map(Duration::from_micros);
        if first_presentation {
            super::runtime::initial_presentation_clock(
                &mut self.renderer_runtime_clock,
                Instant::now(),
            );
        }
        if presentation.clock_advanced {
            self.renderer_clock_pending = false;
        }
        // Every presentation completes one renderer task. A separately in-flight clock advance
        // remains work until its clock-marked output arrives.
        self.renderer_work_pending = self.renderer_clock_pending;
        self.renderer_input_poll_budget = 0;

        if first_presentation {
            let committed_url = self
                .renderer_load_metrics
                .as_ref()
                .map(|metrics| metrics.final_url.as_str())
                .unwrap_or(&presentation.final_url)
                .to_owned();
            self.script_navigation.record_committed(&committed_url);
            self.omnibox_text = self.current_url().unwrap_or(&committed_url).to_owned();
            if !self.processing_background_tab {
                set_window_text(self.controls.address, &self.omnibox_text);
                set_window_text(self.controls.reader, "Reader");
            }
        }
        self.update_active_tab_title(&presentation.title);
        let visual_changed = first_presentation
            || layout_changed
            || images_changed
            || glyph_epoch_changed
            || glyphs_changed
            || glyphs_redefined;
        if visual_changed {
            // Cached scroll pixels belong to the previous immutable visual snapshot.
            // Invalidate before a wheel default action can commit its first frame.
            self.invalidate_benchmark_scroll_surface();
        }
        if presentation.runtime.viewport_scroll_y.is_some() {
            self.pending_history_scroll_y = None;
        }
        self.apply_script_viewport_scroll(presentation.runtime.viewport_scroll_y);
        self.try_restore_history_scroll();
        self.record_benchmark_wheel_decisions(
            presentation.document,
            &presentation.runtime,
            Some(presentation.revision),
            received,
        );
        self.apply_renderer_wheel_scroll(&presentation.runtime);
        if layout_changed {
            self.update_scrollbar();
        }
        if controls_changed {
            // Node IDs can be reused by the next document. Never carry an old EDIT value,
            // selection, or native-input sequence into its first presentation.
            self.recreate_page_controls(!first_presentation);
        }
        if let Some(rejection) = &presentation.runtime.native_text_rejection {
            self.apply_native_text_rejection(presentation.document, rejection);
        }

        let error_count = presentation.runtime.errors.len();
        let script_status = if presentation.runtime.scripts_executed == 0 && error_count == 0 {
            String::new()
        } else {
            format!(
                "  •  JS {} / {} mutations / {error_count} errors",
                presentation.runtime.scripts_executed, presentation.runtime.dom_mutations
            )
        };
        if presentation.runtime.navigation_url.is_none() {
            self.set_status(&format!(
                "HTTP {}  •  isolated renderer{script_status}",
                presentation.status
            ));
        }

        self.paint_installed_presentation(
            &accessibility_update,
            damage,
            visual_changed,
            damage.full_repaint
                || images_changed
                || glyph_epoch_changed
                || glyphs_changed
                || glyphs_redefined,
            first_presentation,
        );
        let presentation_install_time = presentation_install_started.elapsed();
        self.paint_benchmark_wheel_presentation(
            presentation.document,
            presentation.revision,
            layout_changed,
        );
        self.record_presentation_install_incident(first_presentation, presentation_install_time);
        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.presentation_install_time += presentation_install_time;
        }
        let benchmark_completed =
            self.record_renderer_presentation_metrics(&presentation, damage, first_presentation);
        self.acknowledge_renderer_presentation(
            presentation.document,
            presentation.revision,
            true,
            true,
        );
        // Retain the just-completed popstate task's console/metrics before admitting the next
        // queued traversal, which might replace this renderer with a different document.
        if self.apply_queued_history_traversals(
            presentation.document,
            &presentation.runtime.history_actions,
        ) || self.acknowledge_history_traversal(
            presentation.document,
            presentation.runtime.history_traversal_ack,
        ) {
            return;
        }
        self.schedule_script_runtime_wakeup();
        if first_presentation && !self.schedule_benchmark_navigation() {
            self.schedule_benchmark_finish();
        }
        if benchmark_completed {
            self.finish_benchmark_after_completion();
        }
        self.document = Some(presentation.reader);
    }
}
