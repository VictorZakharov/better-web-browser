//! Benchmark accounting for initial and post-load document runtime work.

use super::runtime::PostLoadScriptWork;
use super::*;

pub(super) struct InitialPageMetrics<'a> {
    pub final_url: &'a str,
    pub status: u32,
    pub bytes: u64,
    pub network_time: Duration,
    pub parse_time: Duration,
    pub html_parse_time: Duration,
    pub resource_processing_time: Duration,
    pub script_time: Duration,
    pub style_refresh_time: Duration,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RenderCheckpointMetrics {
    pub style: StyleRefreshStats,
    pub damage: DisplayListDamage,
}

impl RenderCheckpointMetrics {
    pub(super) fn diagnostic(self, outcome: &ScriptOutcome) -> String {
        let root = outcome
            .invalidation
            .root
            .map(|node| node.to_wire().to_string())
            .unwrap_or_else(|| "none".into());
        format!(
            concat!(
                "render checkpoint: mutations={}, root={}, impact={}, invalidated_nodes={}, ",
                "styles_recomputed={}/{}, style_mode={}, layout=full-fallback, ",
                "display_items_invalidated={}, paint_mode={}"
            ),
            outcome.invalidation.mutation_count,
            root,
            outcome.invalidation.impact.labels(),
            self.style.invalidated_nodes,
            self.style.recomputed_styles,
            self.style.total_styles,
            if self.style.full_rebuild {
                "full"
            } else {
                "subtree"
            },
            self.damage.changed_items,
            if self.damage.full_repaint {
                "full"
            } else {
                "region"
            },
        )
    }
}

impl BrowserState {
    pub(super) fn record_initial_script_metrics(
        &mut self,
        page: InitialPageMetrics<'_>,
        outcome: &ScriptOutcome,
    ) {
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        benchmark.network_time = page.network_time;
        benchmark.parse_time = page.parse_time;
        benchmark.html_parse_time = page.html_parse_time;
        benchmark.resource_processing_time = page.resource_processing_time;
        benchmark.script_time = page.script_time;
        benchmark.style_refresh_time = page.style_refresh_time;
        benchmark.status = page.status;
        benchmark.bytes = page.bytes;
        benchmark.final_url = page.final_url.to_string();
        benchmark.script_executed = outcome.executed;
        benchmark.script_mutations = outcome.mutation_count;
        benchmark.script_errors = outcome.errors.clone();
        benchmark.script_console = outcome.console.clone();
        benchmark.script_diagnostics = outcome.diagnostics.clone();
        benchmark.script_runtime_stopped = outcome.runtime_stopped;
    }

    pub(super) fn record_initial_layout_metrics(
        &mut self,
        layout_started: Instant,
        layout_build_time: Duration,
        paint_time: Duration,
    ) {
        let layout_tree_time = self.tabs.active().last_layout_tree_time;
        let layout_finalize_time = self.tabs.active().last_layout_finalize_time;
        let text_measure_count = self.tabs.active().last_text_measure_count;
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        benchmark.layout_time = layout_started.elapsed();
        benchmark.layout_build_time = layout_build_time;
        benchmark.layout_tree_time = layout_tree_time;
        benchmark.layout_finalize_time = layout_finalize_time;
        benchmark.text_measure_count = text_measure_count;
        benchmark.paint_time = paint_time;
        benchmark.page_ready = benchmark.process_started.elapsed();
    }

    pub(super) fn record_post_load_script_outcome(
        &mut self,
        outcome: &ScriptOutcome,
        work: &PostLoadScriptWork,
        style_refresh_time: Duration,
        render: Option<RenderCheckpointMetrics>,
    ) {
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        benchmark.script_time += work.script_time;
        benchmark.style_refresh_time += style_refresh_time;
        benchmark.network_time += work.network_time;
        benchmark.resource_processing_time += work.processing_time;
        benchmark.bytes += work.bytes;
        benchmark.script_executed += outcome.executed;
        benchmark.script_mutations += outcome.mutation_count;
        benchmark
            .script_errors
            .extend(outcome.errors.iter().cloned());
        benchmark
            .script_console
            .extend(outcome.console.iter().cloned());
        benchmark
            .script_diagnostics
            .extend(outcome.diagnostics.iter().cloned());
        benchmark.script_runtime_stopped |= outcome.runtime_stopped;
        if let Some(render) = render {
            benchmark.render_checkpoints += 1;
            benchmark.render_mutations += outcome.invalidation.mutation_count;
            benchmark.invalidated_nodes += render.style.invalidated_nodes;
            benchmark.style_nodes_recomputed += render.style.recomputed_styles;
            benchmark.style_nodes_full_rebuild += render.style.total_styles;
            benchmark.full_style_rebuilds += usize::from(render.style.full_rebuild);
            benchmark.full_layout_rebuilds += 1;
            benchmark.display_items_invalidated += render.damage.changed_items;
            benchmark.full_paint_repaints += usize::from(render.damage.full_repaint);
        }
    }
}
