//! Browser-observed renderer task progress for startup profiling.

use super::*;
use better_web_browser::engine::layout::LayoutStats;

const MAX_RUNTIME_TIMELINE_SAMPLES: usize = 4_096;

#[derive(Default)]
pub(in crate::windows_app) struct RuntimeTimeline {
    samples: Vec<RuntimeSample>,
}

struct RuntimeSample {
    elapsed_ms: f64,
    scripts: usize,
    mutations: usize,
    script_ms: f64,
    script_fetch_wait_ms: f64,
    /// Cumulative renderer style refresh and layout time, to place rendering work in time.
    style_ms: f64,
    layout_ms: f64,
    /// Cumulative styles recomputed and layout work counts (see `LayoutStats`).
    styles_recomputed: usize,
    layout_passes: u64,
    boxes_rebuilt: u64,
    blocks_laid_out: u64,
    items_emitted: u64,
    rendered: bool,
}

/// Cumulative renderer work reported up to one sample.
pub(in crate::windows_app) struct RuntimeWork {
    pub(in crate::windows_app) script: Duration,
    pub(in crate::windows_app) script_fetch: Duration,
    pub(in crate::windows_app) style: Duration,
    pub(in crate::windows_app) layout: Duration,
    pub(in crate::windows_app) layout_work: LayoutStats,
    pub(in crate::windows_app) styles_recomputed: usize,
}

impl RuntimeTimeline {
    pub(in crate::windows_app) fn record(
        &mut self,
        navigation_started: Option<Instant>,
        scripts: usize,
        mutations: usize,
        work: RuntimeWork,
        rendered: bool,
    ) {
        let Some(navigation_started) = navigation_started else {
            return;
        };
        if self.samples.len() >= MAX_RUNTIME_TIMELINE_SAMPLES {
            return;
        }
        self.samples.push(RuntimeSample {
            elapsed_ms: navigation_started.elapsed().as_secs_f64() * 1_000.0,
            scripts,
            mutations,
            script_ms: work.script.as_secs_f64() * 1_000.0,
            script_fetch_wait_ms: work.script_fetch.as_secs_f64() * 1_000.0,
            style_ms: work.style.as_secs_f64() * 1_000.0,
            layout_ms: work.layout.as_secs_f64() * 1_000.0,
            styles_recomputed: work.styles_recomputed,
            layout_passes: work.layout_work.passes,
            boxes_rebuilt: work.layout_work.boxes_rebuilt,
            blocks_laid_out: work.layout_work.blocks_laid_out,
            items_emitted: work.layout_work.items_emitted,
            rendered,
        });
    }

    pub(super) fn to_json(&self) -> String {
        let samples = self
            .samples
            .iter()
            .map(|sample| {
                format!(
                    concat!(
                        "    {{\"elapsed_ms\": {:.3}, \"scripts\": {}, ",
                        "\"mutations\": {}, \"script_ms\": {:.3}, ",
                        "\"script_fetch_wait_ms\": {:.3}, \"style_ms\": {:.3}, ",
                        "\"layout_ms\": {:.3}, \"styles_recomputed\": {}, ",
                        "\"layout_passes\": {}, ",
                        "\"boxes_rebuilt\": {}, \"blocks_laid_out\": {}, ",
                        "\"items_emitted\": {}, \"rendered\": {}}}"
                    ),
                    sample.elapsed_ms,
                    sample.scripts,
                    sample.mutations,
                    sample.script_ms,
                    sample.script_fetch_wait_ms,
                    sample.style_ms,
                    sample.layout_ms,
                    sample.styles_recomputed,
                    sample.layout_passes,
                    sample.boxes_rebuilt,
                    sample.blocks_laid_out,
                    sample.items_emitted,
                    sample.rendered,
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        format!("[\n{samples}\n  ]")
    }
}

/// Style invalidation roots that could not be bounded, summed over a run, by reason.
pub(super) fn invalidation_fallbacks_json(
    fallbacks: better_web_browser::engine::css::InvalidationFallbacks,
) -> String {
    format!(
        concat!(
            "{{\"sibling_combinator\": {}, \"nested_selector\": {}, \"unbounded_rules\": {}, ",
            "\"keyless_descendant\": {}, \"rule_diff_subtrees\": {}}}"
        ),
        fallbacks.sibling_combinator,
        fallbacks.nested_selector,
        fallbacks.unbounded_rules,
        fallbacks.keyless_descendant,
        fallbacks.rule_diff_subtrees,
    )
}

/// Layout work summed over a run: counts and per-phase milliseconds.
pub(super) fn layout_work_json(work: LayoutStats) -> String {
    let ms = |micros: u64| micros as f64 / 1_000.0;
    format!(
        concat!(
            "{{\"passes\": {}, \"boxes_rebuilt\": {}, \"blocks_laid_out\": {}, ",
            "\"subtrees_replayed\": {}, \"items_emitted\": {}, \"items_replayed\": {}, ",
            "\"box_tree_ms\": {:.3}, \"block_layout_ms\": {:.3}, ",
            "\"inline_geometry_ms\": {:.3}, \"overflow_ms\": {:.3}, ",
            "\"paint_order_ms\": {:.3}, \"hit_test_ms\": {:.3}, \"sticky_ms\": {:.3}}}"
        ),
        work.passes,
        work.boxes_rebuilt,
        work.blocks_laid_out,
        work.subtrees_replayed,
        work.items_emitted,
        work.items_replayed,
        ms(work.box_tree_micros),
        ms(work.block_layout_micros),
        ms(work.inline_geometry_micros),
        ms(work.overflow_micros),
        ms(work.paint_order_micros),
        ms(work.hit_test_micros),
        ms(work.sticky_micros),
    )
}
