mod diagnostics;
mod initialization;
mod options;

use super::benchmark_capture::ScrollPaintMetrics;
use super::*;

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
    pub(super) script_runtime_stopped: bool,
    pub(super) finish_scheduled: bool,
    pub(super) renderer_wait_deadline: Option<Instant>,
    pub(super) screenshot: Option<PathBuf>,
    pub(super) scroll_samples: usize,
    pub(super) diagnostic_selectors: Vec<String>,
    pub(super) window_width_dip: i32,
    pub(super) window_height_dip: i32,
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
        post_benchmark_finish(self.window, benchmark.settle);
    }

    pub(super) unsafe fn finish_benchmark(&mut self) {
        // AppContainer profile creation can outlive a short page-settle period on
        // cold hosts. Keep the window responsive while ensuring process metrics
        // include a renderer launch that is still resolving.
        self.finish_renderer_launch();
        if self.renderer_launch_pending {
            let now = Instant::now();
            let should_wait = self.benchmark.as_mut().is_some_and(|benchmark| {
                let deadline = benchmark
                    .renderer_wait_deadline
                    .get_or_insert(now + RENDERER_WAIT_TIMEOUT);
                now < *deadline
            });
            if should_wait {
                post_benchmark_finish(self.window, RENDERER_WAIT_POLL_INTERVAL);
                return;
            }
        }

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
        let renderer_registry = self
            .renderer_registry
            .lock()
            .map(|registry| registry.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        let renderer_snapshots: Vec<_> = renderer_registry
            .renderers
            .iter()
            .filter(|renderer| {
                matches!(
                    renderer.phase,
                    renderer_lifecycle::RendererLifecyclePhase::Running
                        | renderer_lifecycle::RendererLifecyclePhase::Unresponsive
                )
            })
            .filter_map(|renderer| renderer.snapshot.as_ref())
            .collect();
        let renderer_working_set = renderer_snapshots.iter().fold(0_usize, |total, snapshot| {
            total.saturating_add(snapshot.working_set)
        });
        let renderer_private_memory = renderer_snapshots.iter().fold(0_usize, |total, snapshot| {
            total.saturating_add(snapshot.private_memory)
        });
        let renderer_peak_working_set =
            renderer_snapshots.iter().fold(0_usize, |total, snapshot| {
                total.saturating_add(snapshot.peak_working_set)
            });
        let renderer_cpu_ticks = renderer_snapshots.iter().fold(0_u64, |total, snapshot| {
            total.saturating_add(snapshot.cpu_ticks)
        });
        let process_count = 1 + renderer_snapshots.len();
        let elapsed = benchmark.process_started.elapsed();
        let browser_cpu_ticks = process_cpu_ticks()
            .unwrap_or(benchmark.initial_cpu_ticks)
            .saturating_sub(benchmark.initial_cpu_ticks);
        let cpu_ticks = browser_cpu_ticks.saturating_add(renderer_cpu_ticks);
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
                "  \"browser_working_set_bytes\": {},\n",
                "  \"renderer_working_set_bytes\": {},\n",
                "  \"renderer_private_bytes\": {},\n",
                "  \"renderer_peak_working_set_bytes\": {},\n",
                "  \"renderer_cpu_time_ms\": {:.3},\n",
                "  \"cpu_time_ms\": {:.3},\n",
                "  \"average_cpu_percent\": {:.3},\n",
                "  \"process_count\": {},\n",
                "  \"downloaded_bytes\": {},\n",
                "  \"javascript_scripts_executed\": {},\n",
                "  \"javascript_dom_mutations\": {},\n",
                "  \"render_checkpoints\": {},\n",
                "  \"render_mutations_coalesced\": {},\n",
                "  \"invalidated_dom_nodes\": {},\n",
                "  \"style_nodes_recomputed\": {},\n",
                "  \"style_nodes_full_rebuild_equivalent\": {},\n",
                "  \"full_style_rebuilds\": {},\n",
                "  \"full_layout_rebuilds\": {},\n",
                "  \"display_items_invalidated\": {},\n",
                "  \"full_paint_repaints\": {},\n",
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
            memory.working_set.saturating_add(renderer_working_set),
            memory.private_usage.saturating_add(renderer_private_memory),
            memory
                .peak_working_set
                .saturating_add(renderer_peak_working_set),
            memory.working_set,
            renderer_working_set,
            renderer_private_memory,
            renderer_peak_working_set,
            renderer_cpu_ticks as f64 / 10_000.0,
            cpu_seconds * 1_000.0,
            average_cpu,
            process_count,
            metrics.bytes_downloaded,
            benchmark.script_executed,
            benchmark.script_mutations,
            benchmark.render_checkpoints,
            benchmark.render_mutations,
            benchmark.invalidated_nodes,
            benchmark.style_nodes_recomputed,
            benchmark.style_nodes_full_rebuild,
            benchmark.full_style_rebuilds,
            benchmark.full_layout_rebuilds,
            benchmark.display_items_invalidated,
            benchmark.full_paint_repaints,
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

fn post_benchmark_finish(window: Hwnd, delay: Duration) {
    let window = window as isize;
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        unsafe {
            PostMessageW(window as Hwnd, WM_APP_BENCHMARK_FINISH, 0, 0);
        }
    });
}
