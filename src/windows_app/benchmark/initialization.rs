//! Benchmark state initialization and automation completion tracking.

use super::*;

pub(super) fn post_benchmark_finish(window: Hwnd, delay: Duration) {
    let window = window as isize;
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        unsafe {
            PostMessageW(window as Hwnd, WM_APP_BENCHMARK_FINISH, 0, 0);
        }
    });
}

impl BenchmarkRun {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::windows_app) fn new(
        requested_url: String,
        output: PathBuf,
        screenshot: Option<PathBuf>,
        settle: Duration,
        completion_marker: Option<String>,
        scroll_samples: usize,
        early_scroll: bool,
        diagnostic_selectors: Vec<String>,
        window_width_dip: i32,
        window_height_dip: i32,
        process_started: Instant,
    ) -> Self {
        Self {
            requested_url,
            output,
            settle,
            process_started,
            initial_cpu_ticks: process_cpu_ticks().unwrap_or(0),
            window_ready: Duration::ZERO,
            navigation_started: None,
            navigation_targets: Vec::new(),
            navigation_delay: Duration::ZERO,
            navigation_scheduled: false,
            page_ready: Duration::ZERO,
            network_time: Duration::ZERO,
            parse_time: Duration::ZERO,
            html_parse_time: Duration::ZERO,
            resource_processing_time: Duration::ZERO,
            script_time: Duration::ZERO,
            style_refresh_time: Duration::ZERO,
            layout_time: Duration::ZERO,
            layout_build_time: Duration::ZERO,
            layout_tree_time: Duration::ZERO,
            layout_finalize_time: Duration::ZERO,
            text_measure_count: 0,
            text_shape_cache_hits: 0,
            text_shape_cache_misses: 0,
            text_shape_cache_flushes: 0,
            text_shape_cache_entries: 0,
            font_catalog_time: Duration::ZERO,
            font_select_time: Duration::ZERO,
            open_type_shape_time: Duration::ZERO,
            glyph_raster_time: Duration::ZERO,
            presentation_encode_time: Duration::ZERO,
            presentation_decode_time: Duration::ZERO,
            presentation_install_time: Duration::ZERO,
            paint_time: Duration::ZERO,
            status: 0,
            bytes: 0,
            final_url: String::new(),
            error: None,
            script_executed: 0,
            script_executed_at_page_ready: 0,
            script_mutations: 0,
            render_checkpoints: 0,
            render_mutations: 0,
            invalidated_nodes: 0,
            style_nodes_recomputed: 0,
            style_nodes_full_rebuild: 0,
            full_style_rebuilds: 0,
            full_layout_rebuilds: 0,
            display_items_invalidated: 0,
            full_paint_repaints: 0,
            script_errors: Vec::new(),
            script_console: Vec::new(),
            script_diagnostics: Vec::new(),
            script_runtime_stopped: false,
            completion_marker,
            completion_observed: false,
            finish_scheduled: false,
            renderer_wait_deadline: None,
            screenshot,
            scroll_samples,
            early_scroll: early_scroll.then(EarlyScrollTrace::six_seconds),
            scroll_surface: None,
            activity: BenchmarkActivity::default(),
            diagnostic_selectors,
            window_width_dip,
            window_height_dip,
        }
    }

    pub(in crate::windows_app) fn record_script_console(&mut self, messages: &[String]) -> bool {
        let newly_completed = !self.completion_observed
            && self
                .completion_marker
                .as_deref()
                .is_some_and(|marker| messages.iter().any(|message| message.contains(marker)));
        self.completion_observed |= newly_completed;
        self.script_console.extend(messages.iter().cloned());
        newly_completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_marker_finishes_once_and_preserves_console_history() {
        let mut benchmark = BenchmarkRun::new(
            "https://example.test".into(),
            PathBuf::from("report.json"),
            None,
            Duration::from_secs(10),
            Some("__DONE__".into()),
            0,
            false,
            Vec::new(),
            1280,
            720,
            Instant::now(),
        );

        assert!(!benchmark.record_script_console(&["starting".into()]));
        assert!(benchmark.record_script_console(&["log: __DONE__{}".into()]));
        assert!(!benchmark.record_script_console(&["log: __DONE__{}".into()]));
        assert_eq!(
            benchmark.script_console,
            [
                "starting".to_string(),
                "log: __DONE__{}".to_string(),
                "log: __DONE__{}".to_string(),
            ]
        );
    }
}
