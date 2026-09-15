mod diagnostics;
mod early_scroll;
mod filmstrip;
mod initialization;
mod navigation;
mod options;
mod renderer_diagnostics;
mod report;
mod runtime_timeline;
mod video;

use super::benchmark_capture::ScrollPaintMetrics;
use super::*;
use early_scroll::{BenchmarkActivity, EarlyScrollTrace};

pub(super) use options::LaunchOptions;

const RENDERER_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const RENDERER_WAIT_TIMEOUT: Duration = Duration::from_secs(6);

pub(super) struct BenchmarkRun {
    pub(super) requested_url: String,
    pub(super) output: PathBuf,
    pub(super) settle: Duration,
    pub(super) process_started: Instant,
    pub(super) initial_cpu_ticks: u64,
    pub(super) window_ready: Duration,
    pub(super) navigation_started: Option<Instant>,
    pub(super) navigation_targets: Vec<navigation::BenchmarkNavigation>,
    pub(super) navigation_delay: Duration,
    pub(super) navigation_scheduled: bool,
    pub(super) page_ready: Duration,
    pub(super) network_time: Duration,
    pub(super) parse_time: Duration,
    pub(super) html_parse_time: Duration,
    pub(super) resource_processing_time: Duration,
    pub(super) script_time: Duration,
    pub(super) script_fetch_time: Duration,
    pub(super) style_refresh_time: Duration,
    pub(super) layout_time: Duration,
    pub(super) layout_build_time: Duration,
    pub(super) layout_tree_time: Duration,
    pub(super) layout_finalize_time: Duration,
    pub(super) text_measure_count: usize,
    pub(super) text_shape_cache_hits: usize,
    pub(super) text_shape_cache_misses: usize,
    pub(super) text_shape_cache_flushes: usize,
    pub(super) text_shape_cache_entries: usize,
    pub(super) font_catalog_time: Duration,
    pub(super) font_select_time: Duration,
    pub(super) open_type_shape_time: Duration,
    pub(super) glyph_raster_time: Duration,
    pub(super) presentation_encode_time: Duration,
    pub(super) presentation_decode_time: Duration,
    pub(super) presentation_install_time: Duration,
    pub(super) paint_time: Duration,
    pub(super) status: u32,
    pub(super) bytes: u64,
    pub(super) final_url: String,
    pub(super) titles: diagnostics::PageTitles,
    pub(super) error: Option<String>,
    pub(super) script_executed: usize,
    pub(super) script_executed_at_page_ready: usize,
    pub(super) script_mutations: usize,
    pub(super) render_checkpoints: usize,
    pub(super) render_mutations: usize,
    pub(super) invalidated_nodes: usize,
    pub(super) style_nodes_recomputed: usize,
    pub(super) style_nodes_full_rebuild: usize,
    pub(super) full_style_rebuilds: usize,
    pub(super) full_layout_rebuilds: usize,
    pub(super) display_items_invalidated: usize,
    pub(super) full_paint_repaints: usize,
    pub(super) script_errors: Vec<String>,
    pub(super) script_console: Vec<String>,
    pub(super) script_diagnostics: Vec<String>,
    pub(super) media: Option<better_web_browser::renderer_protocol::MediaRuntimeReport>,
    pub(super) video_cadence: video::VideoCadence,
    pub(super) script_runtime_stopped: bool,
    pub(super) runtime_timeline: runtime_timeline::RuntimeTimeline,
    pub(super) completion_marker: Option<String>,
    pub(super) completion_observed: bool,
    pub(super) finish_scheduled: bool,
    pub(super) renderer_wait_deadline: Option<Instant>,
    pub(super) screenshot: Option<PathBuf>,
    pub(super) filmstrip: Option<filmstrip::Filmstrip>,
    pub(super) scroll_samples: usize,
    pub(super) early_scroll: Option<EarlyScrollTrace>,
    pub(super) scroll_surface: Option<benchmark_capture::OffscreenSurface>,
    pub(super) activity: BenchmarkActivity,
    pub(super) diagnostic_selectors: Vec<String>,
    pub(super) window_width_dip: i32,
    pub(super) window_height_dip: i32,
}

impl BrowserState {
    pub(super) fn finish_benchmark_after_completion(&self) {
        initialization::post_benchmark_finish(self.window, Duration::ZERO);
    }

    pub(super) unsafe fn schedule_benchmark_finish(&mut self) {
        if self
            .benchmark
            .as_ref()
            .is_some_and(|benchmark| benchmark.finish_scheduled)
        {
            return;
        }
        let traces_early_scroll = self
            .benchmark
            .as_ref()
            .is_some_and(|benchmark| benchmark.early_scroll.is_some());
        if traces_early_scroll {
            self.note_scroll_activity();
            if let Err(error) = self.prepare_benchmark_scroll_surface()
                && let Some(benchmark) = self.benchmark.as_mut()
            {
                benchmark
                    .error
                    .get_or_insert_with(|| format!("prepare retained scroll surface: {error}"));
            }
        }
        let initial_scroll_y = self.scroll_y;
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        benchmark.finish_scheduled = true;
        if let Some(trace) = benchmark.early_scroll.as_mut() {
            trace.schedule(
                self.window,
                benchmark.settle,
                benchmark.activity,
                initial_scroll_y,
            );
        } else {
            let delay = benchmark
                .filmstrip
                .as_ref()
                .and_then(|filmstrip| filmstrip.remaining())
                .map(|remaining| remaining.max(benchmark.settle))
                .unwrap_or(benchmark.settle);
            initialization::post_benchmark_finish(self.window, delay);
        }
    }
}
