use super::*;

pub(super) struct LaunchOptions {
    pub(super) startup_url: Option<String>,
    pub(super) open_task_manager: bool,
    pub(super) benchmark: Option<BenchmarkRun>,
}

impl LaunchOptions {
    pub(super) fn parse(process_started: Instant) -> Result<Self, String> {
        let mut arguments = std::env::args().skip(1);
        let mut startup_url = None;
        let mut open_task_manager = false;
        let mut benchmark_url = None;
        let mut output = None;
        let mut screenshot = None;
        let mut settle_ms = 2_000_u64;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--benchmark" => {
                    benchmark_url = Some(
                        arguments
                            .next()
                            .ok_or_else(|| "--benchmark requires a URL".to_string())?,
                    );
                }
                "--output" => {
                    output = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or_else(|| "--output requires a path".to_string())?,
                    ));
                }
                "--screenshot" => {
                    screenshot = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or_else(|| "--screenshot requires a path".to_string())?,
                    ));
                }
                "--settle-ms" => {
                    settle_ms = arguments
                        .next()
                        .ok_or_else(|| "--settle-ms requires a number".to_string())?
                        .parse::<u64>()
                        .map_err(|_| "--settle-ms must be a number".to_string())?
                        .clamp(100, 60_000);
                }
                "--task-manager" => open_task_manager = true,
                option if option.starts_with('-') => {
                    return Err(format!("unknown option: {option}"));
                }
                url => startup_url = Some(url.to_string()),
            }
        }

        let benchmark = if let Some(url) = benchmark_url {
            let output = output
                .ok_or_else(|| "benchmark mode requires --output <result.json>".to_string())?;
            startup_url = Some(url.clone());
            Some(BenchmarkRun {
                requested_url: url,
                output,
                settle: Duration::from_millis(settle_ms),
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
            })
        } else {
            if screenshot.is_some() {
                return Err("--screenshot requires --benchmark".to_string());
            }
            None
        };

        Ok(Self {
            startup_url,
            open_task_manager,
            benchmark,
        })
    }
}

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
        let Some(benchmark) = self.benchmark.as_ref() else {
            return;
        };
        let screenshot = benchmark.screenshot.clone();
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let viewport_width = client.right.max(1) as f32 / self.page_scale();
        let viewport_height = self.viewport_height().max(1) as f32 / self.page_scale();
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

    pub(super) unsafe fn capture_screenshot(
        &mut self,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let mut client: Rect = std::mem::zeroed();
        if GetClientRect(self.window, &mut client) == 0 {
            return Err(last_error("measure benchmark capture"));
        }
        let width = client.right.max(1);
        let height = client.bottom.max(1);
        let byte_len = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "benchmark capture is too large".to_string())?;

        let window_dc = GetDC(self.window);
        if window_dc.is_null() {
            return Err(last_error("open benchmark capture surface"));
        }
        let memory_dc = CreateCompatibleDC(window_dc);
        if memory_dc.is_null() {
            ReleaseDC(self.window, window_dc);
            return Err(last_error("create benchmark capture surface"));
        }
        let info = BitmapInfo {
            header: BitmapInfoHeader {
                size: size_of::<BitmapInfoHeader>() as u32,
                width,
                height: -height,
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: byte_len.min(u32::MAX as usize) as u32,
                x_pixels_per_meter: 0,
                y_pixels_per_meter: 0,
                colors_used: 0,
                colors_important: 0,
            },
            colors: [0],
        };
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(window_dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        if bitmap.is_null() || pixels.is_null() {
            DeleteDC(memory_dc);
            ReleaseDC(self.window, window_dc);
            return Err(last_error("allocate benchmark capture bitmap"));
        }

        let previous = SelectObject(memory_dc, bitmap);
        self.paint_surface(memory_dc, &client);
        if let Some(fonts) = self.fonts.as_ref() {
            SelectObject(memory_dc, fonts.ui);
            SetTextColor(memory_dc, CHROME_THEME.text);
            SetBkMode(memory_dc, TRANSPARENT);
            let mut address_rect = self
                .chrome
                .address_frame
                .inset(self.scale(16), self.scale(1));
            let address = window_text(self.controls.address);
            draw_text_in_rect(
                memory_dc,
                &address,
                &mut address_rect,
                DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }
        let bgra = std::slice::from_raw_parts(pixels.cast::<u8>(), byte_len);
        let mut rgba = Vec::with_capacity(byte_len);
        for pixel in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }

        if !previous.is_null() {
            SelectObject(memory_dc, previous);
        }
        DeleteObject(bitmap);
        DeleteDC(memory_dc);
        ReleaseDC(self.window, window_dc);

        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create screenshot directory: {error}"))?;
        }
        image::save_buffer(
            path,
            &rgba,
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )
        .map_err(|error| format!("write screenshot: {error}"))
    }
}
