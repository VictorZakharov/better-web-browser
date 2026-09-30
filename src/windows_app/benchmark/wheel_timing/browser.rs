//! Hooks at validated verdict receipt and that verdict's actual native motion paint.
use super::*;
use crate::windows_app::*;
use better_web_browser::renderer_protocol::RuntimeReport;

impl BrowserState {
    pub(in crate::windows_app) fn record_benchmark_animation_frame(&mut self, initial: bool) {
        if self.processing_background_tab {
            return;
        }
        let y = viewport_css_position(self.scroll_y, self.page_scale());
        if let Some(document) = self.navigation.active_document()
            && let Some(benchmark) = self.benchmark.as_mut()
            && !benchmark.wheel_trace.samples.is_empty()
        {
            benchmark
                .wheel_trace
                .animation
                .record(document, y, initial, Instant::now());
        }
    }

    pub(in crate::windows_app) fn record_benchmark_scroll_reversal(&mut self, direction: i32) {
        self.record_benchmark_scroll_interruption(Some(direction));
    }

    pub(in crate::windows_app) fn record_benchmark_scroll_interruption(
        &mut self,
        direction: Option<i32>,
    ) {
        if self.processing_background_tab {
            return;
        }
        if let Some(document) = self.navigation.active_document()
            && let Some(benchmark) = self.benchmark.as_mut()
        {
            benchmark
                .wheel_trace
                .interrupt_viewport(document, direction);
        }
    }

    pub(in crate::windows_app) fn record_benchmark_wheel_decisions(
        &mut self,
        document: DocumentId,
        report: &RuntimeReport,
        revision: Option<u64>,
        received: Instant,
    ) {
        if self.processing_background_tab {
            return;
        }
        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.wheel_trace.retire_other_documents(document);
            benchmark.wheel_trace.acknowledge(
                document,
                &report.wheel_acknowledgements,
                revision,
                received,
            );
        }
    }

    pub(in crate::windows_app) fn benchmark_wheel_needs_viewport_paint(&self) -> bool {
        self.navigation.active_document().is_some_and(|document| {
            self.benchmark
                .as_ref()
                .is_some_and(|benchmark| benchmark.wheel_trace.wants_viewport_paint(document))
        })
    }

    pub(in crate::windows_app) unsafe fn paint_benchmark_wheel_viewport(&mut self) {
        let Some(document) = self.navigation.active_document() else {
            return;
        };
        self.paint_benchmark_wheel(document, None);
    }

    pub(in crate::windows_app) fn resolve_benchmark_wheel_viewport_request(
        &mut self,
        new_motion: bool,
    ) {
        if self.processing_background_tab {
            return;
        }
        let y = viewport_css_position(self.scroll_y, self.page_scale());
        if let Some(document) = self.navigation.active_document()
            && let Some(benchmark) = self.benchmark.as_mut()
        {
            benchmark.wheel_trace.viewport_position(document, y, false);
            benchmark.wheel_trace.viewport_request(document, new_motion);
        }
    }

    pub(in crate::windows_app) unsafe fn paint_benchmark_wheel_presentation(
        &mut self,
        document: DocumentId,
        revision: u64,
        changed: bool,
    ) {
        if self.processing_background_tab {
            return;
        }
        if !self
            .benchmark
            .as_ref()
            .is_some_and(|benchmark| benchmark.wheel_trace.wants_nested_paint(document, revision))
        {
            return;
        }
        if !changed {
            self.benchmark
                .as_mut()
                .unwrap()
                .wheel_trace
                .no_motion(document, Some(revision));
            return;
        }
        // The nested-scroll snapshot changed retained content, not the shell viewport.
        // Do not reuse exposed-strip pixels from an older renderer installation.
        self.invalidate_benchmark_scroll_surface();
        self.paint_benchmark_wheel(document, Some(revision));
    }

    unsafe fn paint_benchmark_wheel(&mut self, document: DocumentId, revision: Option<u64>) {
        let result = self.paint_benchmark_frame_with_path();
        let y = viewport_css_position(self.scroll_y, self.page_scale());
        let now = Instant::now();
        if let Some(benchmark) = self.benchmark.as_mut() {
            match result {
                Ok((_, full_repaint)) => {
                    if revision.is_none() {
                        benchmark.wheel_trace.viewport_position(document, y, true);
                    }
                    benchmark
                        .wheel_trace
                        .painted(document, revision, now, full_repaint)
                }
                Err(error) => {
                    benchmark.wheel_trace.paint_failed(document, revision);
                    benchmark
                        .error
                        .get_or_insert_with(|| format!("wheel retained paint failed: {error}"));
                }
            }
        }
    }
}
