//! Benchmark accounting for initial and post-load document runtime work.

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
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        benchmark.layout_time = layout_started.elapsed();
        benchmark.layout_build_time = layout_build_time;
        benchmark.layout_tree_time = self.last_layout_tree_time;
        benchmark.layout_finalize_time = self.last_layout_finalize_time;
        benchmark.text_measure_count = self.last_text_measure_count;
        benchmark.paint_time = paint_time;
        benchmark.page_ready = benchmark.process_started.elapsed();
    }

    pub(super) fn record_post_load_script_outcome(
        &mut self,
        outcome: &ScriptOutcome,
        script_time: Duration,
        style_refresh_time: Duration,
        network_time: Duration,
        resource_processing_time: Duration,
        bytes: u64,
    ) {
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        benchmark.script_time += script_time;
        benchmark.style_refresh_time += style_refresh_time;
        benchmark.network_time += network_time;
        benchmark.resource_processing_time += resource_processing_time;
        benchmark.bytes += bytes;
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
    }
}
