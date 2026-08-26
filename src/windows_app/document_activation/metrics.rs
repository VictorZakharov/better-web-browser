use super::*;
use better_web_browser::renderer_protocol::{PageLoadReport, RuntimeReport};

impl BrowserState {
    pub(super) fn network_incident(page: &LoadedPage) -> String {
        format!(
            "network complete: {} (HTTP {}, {} bytes, {:.1} ms)",
            page.final_url,
            page.status,
            page.bytes,
            page.network_time.as_secs_f64() * 1_000.0
        )
    }

    pub(super) fn record_renderer_presentation_incident(
        &mut self,
        presentation: &RendererPresentation,
        first: bool,
    ) {
        self.incidents.presentations = self.incidents.presentations.saturating_add(1);
        self.incidents.record(
            "present",
            format!(
                "revision {} ({}, {} items, {} accessibility nodes, {} console, {} errors)",
                presentation.revision,
                if first { "first" } else { "update" },
                presentation.layout.items.len(),
                presentation.accessibility.nodes.len(),
                presentation.runtime.console.len(),
                presentation.runtime.errors.len()
            ),
        );
    }

    pub(super) fn record_renderer_submission(
        &mut self,
        document: better_web_browser::renderer_protocol::DocumentId,
        bytes: u32,
    ) {
        self.incidents.record(
            "renderer",
            format!("submitted document {} ({bytes} bytes)", document.get()),
        );
    }

    pub(super) fn record_presentation_install_incident(&mut self, first: bool, duration: Duration) {
        if first || duration >= Duration::from_millis(50) {
            self.incidents.record(
                "ui-install",
                format!(
                    "{} presentation installed in {:.1} ms",
                    if first { "first" } else { "updated" },
                    duration.as_secs_f64() * 1_000.0
                ),
            );
        }
    }

    pub(in crate::windows_app) fn record_renderer_runtime_metrics(
        &mut self,
        runtime: &RuntimeReport,
        load: PageLoadReport,
        reached_page_ready: bool,
    ) -> bool {
        let script_time = Duration::from_micros(load.script_micros);
        let style_time = Duration::from_micros(load.style_micros);
        let layout_time = Duration::from_micros(load.layout_micros);
        self.record_performance_activity(PerformanceActivity::Script, script_time);
        self.record_performance_activity(PerformanceActivity::Style, style_time);
        self.record_performance_activity(PerformanceActivity::Layout, layout_time);
        for message in runtime.console.iter().take(16) {
            self.incidents.record("console", message);
        }
        for message in runtime.errors.iter().take(16) {
            self.incidents.record("script-error", message);
        }
        for message in runtime.diagnostics.iter().take(16) {
            self.incidents.record("script-diag", message);
        }
        let Some(benchmark) = self.benchmark.as_mut() else {
            return false;
        };
        benchmark.script_time += script_time;
        benchmark.style_refresh_time += style_time;
        benchmark.layout_time += layout_time;
        benchmark.layout_build_time += layout_time;
        benchmark.layout_tree_time += layout_time;
        benchmark.text_measure_count = benchmark
            .text_measure_count
            .saturating_add(load.text_measure_count as usize);
        benchmark.text_shape_cache_hits = benchmark
            .text_shape_cache_hits
            .saturating_add(load.text_shape_cache_hits as usize);
        benchmark.text_shape_cache_misses = benchmark
            .text_shape_cache_misses
            .saturating_add(load.text_shape_cache_misses as usize);
        benchmark.text_shape_cache_flushes = benchmark
            .text_shape_cache_flushes
            .saturating_add(load.text_shape_cache_flushes as usize);
        benchmark.text_shape_cache_entries = load.text_shape_cache_entries as usize;
        benchmark.font_catalog_time += Duration::from_micros(load.font_catalog_micros);
        benchmark.font_select_time += Duration::from_micros(load.font_select_micros);
        benchmark.open_type_shape_time += Duration::from_micros(load.open_type_shape_micros);
        benchmark.glyph_raster_time += Duration::from_micros(load.glyph_raster_micros);
        benchmark.presentation_encode_time +=
            Duration::from_micros(load.presentation_encode_micros);
        benchmark.presentation_decode_time +=
            Duration::from_micros(load.presentation_decode_micros);
        benchmark.script_executed = benchmark
            .script_executed
            .saturating_add(runtime.scripts_executed as usize);
        if reached_page_ready {
            benchmark.script_executed_at_page_ready = benchmark.script_executed;
        }
        benchmark.script_mutations = benchmark
            .script_mutations
            .saturating_add(runtime.dom_mutations as usize);
        benchmark
            .script_errors
            .extend(runtime.errors.iter().cloned());
        let benchmark_completed = benchmark.record_script_console(&runtime.console);
        benchmark
            .script_diagnostics
            .extend(runtime.diagnostics.iter().cloned());
        benchmark.script_runtime_stopped |= runtime.runtime_stopped;
        benchmark_completed
    }

    pub(super) fn record_renderer_presentation_metrics(
        &mut self,
        presentation: &RendererPresentation,
        damage: DisplayListDamage,
        first: bool,
    ) -> bool {
        let load = first.then(|| self.renderer_load_metrics.take()).flatten();
        let reached_page_ready = load.is_some();
        if let (Some(load), Some(benchmark)) = (load, self.benchmark.as_mut()) {
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
        let benchmark_completed = self.record_renderer_runtime_metrics(
            &presentation.runtime,
            presentation.load,
            reached_page_ready,
        );
        let Some(benchmark) = self.benchmark.as_mut() else {
            return false;
        };
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
        benchmark_completed
    }
}
