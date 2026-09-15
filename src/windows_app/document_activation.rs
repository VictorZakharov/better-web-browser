//! Commits browser-fetched bytes to the page-owning renderer and installs validated output.

mod metrics;
mod submission;
mod video;

use super::browser_navigation::HistoryMode;
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
    pub(super) status: u16,
    pub(super) content_type: String,
    pub(super) bytes: u64,
    pub(super) network_time: Duration,
}

impl LoadedPage {
    pub(super) fn home() -> Self {
        Self {
            stream: None,
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
        let presentation_install_started = Instant::now();
        self.renderer_revision = presentation.revision;
        self.record_renderer_presentation_incident(&presentation, first_presentation);

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
            self.begin_document_navigation(url.to_string(), HistoryMode::Script);
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
        let controls_changed = first_presentation || self.page_layout.forms != next_layout.forms;
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
        let images_changed = !presentation.images.is_empty();
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
        self.reader_url.clone_from(&presentation.final_url);
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
        }
        self.apply_same_document_history_updates(&presentation.runtime.history_updates);
        self.update_active_tab_title(&presentation.title);
        self.apply_script_viewport_scroll(presentation.runtime.viewport_scroll_y);
        self.queue_css_wheel_scroll(presentation.runtime.viewport_wheel_delta_y);
        if layout_changed {
            self.update_scrollbar();
        }
        if controls_changed {
            self.recreate_page_controls();
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
        self.set_status(&format!(
            "HTTP {}  •  isolated renderer{script_status}",
            presentation.status
        ));

        let visual_changed = first_presentation
            || layout_changed
            || images_changed
            || glyph_epoch_changed
            || glyphs_changed
            || glyphs_redefined;
        if !self.processing_background_tab && visual_changed {
            self.refresh_accessibility_document(&accessibility_update);
            let paint_started = Instant::now();
            if damage.full_repaint
                || images_changed
                || glyph_epoch_changed
                || glyphs_changed
                || glyphs_redefined
            {
                let mut client: Rect = std::mem::zeroed();
                GetClientRect(self.window, &mut client);
                let content = Rect {
                    left: 0,
                    top: self.toolbar_height(),
                    right: client.right,
                    bottom: (client.bottom - self.status_height()).max(self.toolbar_height()),
                };
                InvalidateRect(self.window, &content, 0);
            } else if let Some(rect) = damage.rect {
                let dirty = screen_rect(
                    rect,
                    self.scroll_y,
                    self.toolbar_height(),
                    self.page_scale(),
                );
                InvalidateRect(self.window, &dirty, 0);
            }
            UpdateWindow(self.window);
            if first_presentation && let Some(benchmark) = self.benchmark.as_mut() {
                benchmark.paint_time = paint_started.elapsed();
            }
        } else if !self.processing_background_tab {
            self.refresh_accessibility_document(&accessibility_update);
        }
        let presentation_install_time = presentation_install_started.elapsed();
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
