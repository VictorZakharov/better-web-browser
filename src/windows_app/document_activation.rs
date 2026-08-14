//! Commits a loaded document and establishes its first-paint runtime state.

use super::browser_navigation::HistoryMode;
use super::*;

pub(super) struct LoadedPage {
    pub page: Page,
    pub html: String,
    pub final_url: String,
    pub status: u32,
    pub bytes: u64,
    pub network_time: Duration,
    pub parse_time: Duration,
    pub html_parse_time: Duration,
    pub resource_processing_time: Duration,
    pub loaded_resources: HashSet<PageResource>,
    pub remaining_resource_budget: u64,
}

pub(super) struct LoadMessage {
    pub generation: u64,
    pub result: Result<LoadedPage, String>,
}

impl BrowserState {
    pub(super) unsafe fn finish_navigation(&mut self, message: LoadMessage) {
        if message.generation != self.generation {
            return;
        }
        self.loading = false;
        match message.result {
            Ok(page) => self.activate_loaded_page(page),
            Err(error) => {
                self.set_status(&format!("Load failed: {error}"));
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.error = Some(error);
                    benchmark.page_ready = benchmark.process_started.elapsed();
                }
                self.schedule_benchmark_finish();
            }
        }
    }

    unsafe fn activate_loaded_page(&mut self, mut page: LoadedPage) {
        self.destroy_page_controls();
        self.image_bitmaps.clear();
        self.dynamic_fonts.clear();
        self.web_fonts.clear();
        self.page = page.page;
        self.loaded_page_resources = std::mem::take(&mut page.loaded_resources);
        self.page_resource_budget = page.remaining_resource_budget;
        self.document = None;
        self.reader_html = page.html.clone();
        self.reader_url = page.final_url.clone();
        self.script_navigation.record_committed(&page.final_url);
        self.surface = Surface::Page;
        set_window_text(self.controls.reader, "Reader");
        self.scroll_y = 0;

        let client = Arc::clone(&self.http_client);
        let mut total_bytes = page.bytes;
        let mut additional_network_time = Duration::ZERO;
        let mut additional_processing_time = Duration::ZERO;
        let mut resource_budget = self.page_resource_budget;
        let script_started = Instant::now();
        let (mut runtime, mut script_outcome) = {
            let mut dynamic_script_loader = |url: &str| -> Result<String, String> {
                let request_started = Instant::now();
                let response = client.get(url);
                additional_network_time += request_started.elapsed();
                let response = response?;
                if !response.is_success() {
                    return Err(format!("server returned HTTP {}", response.status));
                }
                let size = response.body.len() as u64;
                if size > resource_budget {
                    return Err("page resource budget was exhausted".into());
                }
                let processing_started = Instant::now();
                let code = winhttp::decode_text(&response.body, response.content_type.as_deref());
                additional_processing_time += processing_started.elapsed();
                total_bytes += size;
                resource_budget -= size;
                Ok(code)
            };
            self.page
                .start_first_paint_script_runtime_with_loader(&mut dynamic_script_loader)
        };
        let script_time = script_started
            .elapsed()
            .saturating_sub(additional_network_time);

        for cookie in &script_outcome.cookie_updates {
            if let Err(error) = client.set_cookie(&page.final_url, cookie) {
                script_outcome
                    .errors
                    .push(format!("document.cookie: {error}"));
            }
        }

        let mut style_refresh_time = Duration::ZERO;
        if script_outcome.runtime_stopped {
            if let Some(runtime) = runtime.as_mut() {
                runtime.cancel_document();
            }
            runtime = None;
            let fallback_parse_started = Instant::now();
            self.page = Page::parse(&page.html, &page.final_url);
            page.html_parse_time += fallback_parse_started.elapsed();
            self.loaded_page_resources.clear();
        } else {
            let style_refresh_started = Instant::now();
            self.page
                .refresh_resources(self.current_style_viewport_width());
            style_refresh_time += style_refresh_started.elapsed();
            load_page_resources(
                &client,
                &mut self.page,
                &mut self.loaded_page_resources,
                &mut resource_budget,
                &mut total_bytes,
                &mut additional_network_time,
                &mut additional_processing_time,
            );
        }
        self.page_resource_budget = resource_budget;
        page.bytes = total_bytes;
        page.network_time += additional_network_time;
        page.resource_processing_time += additional_processing_time;

        if let Some(navigation_url) = script_outcome.navigation_url.clone()
            && navigation_url != page.final_url
            && self.allow_script_navigation(&navigation_url)
        {
            if let Some(runtime) = runtime.as_mut() {
                runtime.cancel_document();
            }
            self.begin_navigation(navigation_url, HistoryMode::Script);
            return;
        }

        self.web_fonts.register(&self.page.fonts);
        if let Some(current) = self.history.get_mut(self.history_index) {
            *current = page.final_url.clone();
        }
        set_window_text(self.controls.address, &page.final_url);
        set_window_text(
            self.window,
            &format!("{} \u{2014} {PRODUCT_NAME}", self.page.title),
        );
        let layout_started = Instant::now();
        self.rebuild_layout();
        let layout_build_time = layout_started.elapsed();
        self.record_initial_script_metrics(
            super::runtime_metrics::InitialPageMetrics {
                final_url: &page.final_url,
                status: page.status,
                bytes: page.bytes,
                network_time: page.network_time,
                parse_time: page.parse_time,
                html_parse_time: page.html_parse_time,
                resource_processing_time: page.resource_processing_time,
                script_time,
                style_refresh_time,
            },
            &script_outcome,
        );

        let script_status = if script_outcome.executed == 0 && script_outcome.errors.is_empty() {
            String::new()
        } else {
            format!(
                "  \u{2022}  JS {} / {} mutations / {} errors",
                script_outcome.executed,
                script_outcome.mutation_count,
                script_outcome.errors.len()
            )
        };
        self.set_status(&format!(
            "HTTP {}  \u{2022}  {}  \u{2022}  network {}  \u{2022}  parse {}{}",
            page.status,
            format_bytes(page.bytes),
            format_duration(page.network_time),
            format_duration(page.parse_time),
            script_status
        ));
        let paint_started = Instant::now();
        InvalidateRect(self.window, null(), 0);
        UpdateWindow(self.window);
        let paint_time = paint_started.elapsed();
        self.record_initial_layout_metrics(layout_started, layout_build_time, paint_time);
        self.install_script_runtime(runtime);
        self.begin_async_scripts();
        let deferred_resources = self.unloaded_deferred_resources();
        self.schedule_benchmark_finish();
        self.begin_deferred_resources(deferred_resources);
    }
}
