mod diagnostics;
mod options;

use super::benchmark_capture::ScrollPaintMetrics;
use super::*;

pub(super) use options::LaunchOptions;

pub(super) struct BenchmarkRun {
    pub(super) requested_url: String,
    pub(super) output: PathBuf,
    pub(super) settle: Duration,
    pub(super) process_started: Instant,
    pub(super) initial_cpu_ticks: u64,
    pub(super) window_ready: Duration,
    pub(super) navigation_started: Option<Instant>,
    pub(super) page_ready: Duration,
    pub(super) network_time: Duration,
    pub(super) parse_time: Duration,
    pub(super) html_parse_time: Duration,
    pub(super) resource_processing_time: Duration,
    pub(super) script_time: Duration,
    pub(super) style_refresh_time: Duration,
    pub(super) layout_time: Duration,
    pub(super) layout_build_time: Duration,
    pub(super) layout_tree_time: Duration,
    pub(super) layout_finalize_time: Duration,
    pub(super) text_measure_count: usize,
    pub(super) paint_time: Duration,
    pub(super) status: u32,
    pub(super) bytes: u64,
    pub(super) final_url: String,
    pub(super) error: Option<String>,
    pub(super) script_executed: usize,
    pub(super) script_mutations: usize,
    pub(super) script_errors: Vec<String>,
    pub(super) script_console: Vec<String>,
    pub(super) script_diagnostics: Vec<String>,
    pub(super) script_runtime_stopped: bool,
    pub(super) finish_scheduled: bool,
    pub(super) screenshot: Option<PathBuf>,
    pub(super) scroll_samples: usize,
    pub(super) diagnostic_selectors: Vec<String>,
    pub(super) window_width_dip: i32,
    pub(super) window_height_dip: i32,
}

impl BenchmarkRun {
    #[allow(clippy::too_many_arguments)]
    fn new(
        requested_url: String,
        output: PathBuf,
        screenshot: Option<PathBuf>,
        settle: Duration,
        scroll_samples: usize,
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
            paint_time: Duration::ZERO,
            status: 0,
            bytes: 0,
            final_url: String::new(),
            error: None,
            script_executed: 0,
            script_mutations: 0,
            script_errors: Vec::new(),
            script_console: Vec::new(),
            script_diagnostics: Vec::new(),
            script_runtime_stopped: false,
            finish_scheduled: false,
            screenshot,
            scroll_samples,
            diagnostic_selectors,
            window_width_dip,
            window_height_dip,
        }
    }
}

impl BrowserState {
    pub(super) unsafe fn schedule_benchmark_finish(&mut self) {
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        if benchmark.finish_scheduled {
            return;
        }
        benchmark.finish_scheduled = true;
        let delay = benchmark.settle;
        let window = self.window as isize;
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            unsafe {
                PostMessageW(window as Hwnd, WM_APP_BENCHMARK_FINISH, 0, 0);
            }
        });
    }

    pub(super) unsafe fn finish_benchmark(&mut self) {
        let scroll_sample_count = self
            .benchmark
            .as_ref()
            .map(|benchmark| benchmark.scroll_samples)
            .unwrap_or(0);
        let scroll_paint = match self.measure_scroll_paints(scroll_sample_count) {
            Ok(metrics) => metrics,
            Err(error) => {
                self.set_status(&format!("Failed to measure scrolling: {error}"));
                ScrollPaintMetrics::default()
            }
        };
        let Some(benchmark) = self.benchmark.as_ref() else {
            return;
        };
        let screenshot = benchmark.screenshot.clone();
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let viewport_width = client.right.max(1) as f32 / self.page_scale();
        let viewport_height = self.viewport_height().max(1) as f32 / self.page_scale();
        let style_viewport_width = if self.media_viewport_width > 0.0 {
            self.media_viewport_width
        } else {
            viewport_width
        };
        let diagnostics =
            diagnostics::collect(self, &benchmark.diagnostic_selectors, style_viewport_width);
        let memory = process_memory();
        let elapsed = benchmark.process_started.elapsed();
        let cpu_ticks = process_cpu_ticks()
            .unwrap_or(benchmark.initial_cpu_ticks)
            .saturating_sub(benchmark.initial_cpu_ticks);
        let cpu_seconds = cpu_ticks as f64 / 10_000_000.0;
        let processors = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1) as f64;
        let average_cpu = if elapsed.is_zero() {
            0.0
        } else {
            cpu_seconds / elapsed.as_secs_f64() / processors * 100.0
        };
        let navigation_ms = benchmark
            .navigation_started
            .map(|started| {
                benchmark
                    .page_ready
                    .saturating_sub(started.duration_since(benchmark.process_started))
            })
            .unwrap_or_default()
            .as_secs_f64()
            * 1_000.0;
        let metrics = self.metrics.snapshot();
        let script_errors = format!(
            "[{}]",
            benchmark
                .script_errors
                .iter()
                .map(|error| json_string(error))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let script_console = format!(
            "[{}]",
            benchmark
                .script_console
                .iter()
                .map(|message| json_string(message))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let script_diagnostics = format!(
            "[{}]",
            benchmark
                .script_diagnostics
                .iter()
                .map(|message| json_string(message))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let json = format!(
            concat!(
                "{{\n",
                "  \"browser\": {},\n",
                "  \"requested_url\": {},\n",
                "  \"final_url\": {},\n",
                "  \"error\": {},\n",
                "  \"http_status\": {},\n",
                "  \"viewport_width_css_px\": {:.3},\n",
                "  \"viewport_height_css_px\": {:.3},\n",
                "  \"window_ready_ms\": {:.3},\n",
                "  \"page_ready_ms\": {:.3},\n",
                "  \"navigation_ms\": {:.3},\n",
                "  \"network_ms\": {:.3},\n",
                "  \"parse_ms\": {:.3},\n",
                "  \"html_parse_ms\": {:.3},\n",
                "  \"resource_processing_ms\": {:.3},\n",
                "  \"javascript_ms\": {:.3},\n",
                "  \"style_refresh_ms\": {:.3},\n",
                "  \"layout_and_paint_ms\": {:.3},\n",
                "  \"layout_build_ms\": {:.3},\n",
                "  \"layout_tree_ms\": {:.3},\n",
                "  \"layout_finalize_ms\": {:.3},\n",
                "  \"text_measure_count\": {},\n",
                "  \"paint_ms\": {:.3},\n",
                "  \"scroll_paint_samples\": {},\n",
                "  \"average_scroll_paint_ms\": {:.3},\n",
                "  \"maximum_scroll_paint_ms\": {:.3},\n",
                "  \"settle_ms\": {},\n",
                "  \"working_set_bytes\": {},\n",
                "  \"private_bytes\": {},\n",
                "  \"peak_working_set_bytes\": {},\n",
                "  \"cpu_time_ms\": {:.3},\n",
                "  \"average_cpu_percent\": {:.3},\n",
                "  \"process_count\": 1,\n",
                "  \"downloaded_bytes\": {},\n",
                "  \"javascript_scripts_executed\": {},\n",
                "  \"javascript_dom_mutations\": {},\n",
                "  \"javascript_errors\": {},\n",
                "  \"javascript_console\": {},\n",
                "  \"javascript_diagnostics\": {},\n",
                "  \"javascript_runtime_stopped\": {},\n",
                "  \"diagnostics\": {},\n",
                "  \"retained_draw_items\": {}\n",
                "}}\n"
            ),
            json_string(BENCHMARK_ID),
            json_string(&benchmark.requested_url),
            json_string(&benchmark.final_url),
            benchmark
                .error
                .as_deref()
                .map(json_string)
                .unwrap_or_else(|| "null".into()),
            benchmark.status,
            viewport_width,
            viewport_height,
            benchmark.window_ready.as_secs_f64() * 1_000.0,
            benchmark.page_ready.as_secs_f64() * 1_000.0,
            navigation_ms,
            benchmark.network_time.as_secs_f64() * 1_000.0,
            benchmark.parse_time.as_secs_f64() * 1_000.0,
            benchmark.html_parse_time.as_secs_f64() * 1_000.0,
            benchmark.resource_processing_time.as_secs_f64() * 1_000.0,
            benchmark.script_time.as_secs_f64() * 1_000.0,
            benchmark.style_refresh_time.as_secs_f64() * 1_000.0,
            benchmark.layout_time.as_secs_f64() * 1_000.0,
            benchmark.layout_build_time.as_secs_f64() * 1_000.0,
            benchmark.layout_tree_time.as_secs_f64() * 1_000.0,
            benchmark.layout_finalize_time.as_secs_f64() * 1_000.0,
            benchmark.text_measure_count,
            benchmark.paint_time.as_secs_f64() * 1_000.0,
            scroll_paint.samples,
            scroll_paint.average.as_secs_f64() * 1_000.0,
            scroll_paint.maximum.as_secs_f64() * 1_000.0,
            benchmark.settle.as_millis(),
            memory.working_set,
            memory.private_usage,
            memory.peak_working_set,
            cpu_seconds * 1_000.0,
            average_cpu,
            metrics.bytes_downloaded,
            benchmark.script_executed,
            benchmark.script_mutations,
            script_errors,
            script_console,
            script_diagnostics,
            benchmark.script_runtime_stopped,
            diagnostics,
            metrics.retained_draw_items,
        );
        let write_result = benchmark
            .output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::write(&benchmark.output, json));
        if let Err(error) = write_result {
            self.set_status(&format!("Failed to write benchmark: {error}"));
        }
        if let Some(path) = screenshot
            && let Err(error) = self.capture_screenshot(&path)
        {
            self.set_status(&format!("Failed to capture benchmark: {error}"));
        }
        DestroyWindow(self.window);
    }
}
