//! UI-thread ownership and wakeups for a document's retained JavaScript realm.
//! Boa contexts remain on the thread that creates them. Win32 posts `WM_TIMER` to the owning
//! window's thread, so the document, realm, style invalidation, layout, and painting all stay on
//! one owner without cross-thread runtime transfer.
//! Platform references:
//! - <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-settimer>
//! - <https://learn.microsoft.com/windows/win32/winmsg/wm-timer>

use super::*;
const USER_TIMER_MINIMUM_MS: u128 = 10;
const USER_TIMER_MAXIMUM_MS: u128 = 0x7fff_ffff;

pub(super) struct PostLoadScriptWork {
    pub script_time: Duration,
    pub network_time: Duration,
    pub processing_time: Duration,
    pub bytes: u64,
    pub resource_budget: u64,
}

impl BrowserState {
    pub(super) unsafe fn current_style_viewport_width(&self) -> f32 {
        if self.media_viewport_width > 0.0 {
            return self.media_viewport_width;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        client.right.max(1) as f32 / self.page_scale()
    }

    pub(super) unsafe fn cancel_script_runtime(&mut self) {
        if !self.window.is_null() {
            KillTimer(self.window, ID_SCRIPT_RUNTIME_TIMER);
        }
        if let Some(mut runtime) = self.script_runtime.take() {
            runtime.cancel_document();
        }
        self.script_runtime_clock = None;
    }

    pub(super) unsafe fn install_script_runtime(&mut self, runtime: Option<ScriptRuntime>) {
        self.cancel_script_runtime();
        self.script_runtime = runtime;
        self.script_runtime_clock = self.script_runtime.as_ref().map(|_| Instant::now());
        self.schedule_script_runtime_wakeup();
    }

    unsafe fn schedule_script_runtime_wakeup(&mut self) {
        KillTimer(self.window, ID_SCRIPT_RUNTIME_TIMER);
        let next_delay = self
            .script_runtime
            .as_mut()
            .and_then(ScriptRuntime::next_timer_delay);
        let Some(next_delay) = next_delay else {
            return;
        };
        if SetTimer(
            self.window,
            ID_SCRIPT_RUNTIME_TIMER,
            win32_timer_delay_ms(next_delay),
            null(),
        ) == 0
        {
            self.set_status("JavaScript timer scheduling failed");
        }
    }

    pub(super) unsafe fn pump_script_runtime(&mut self) {
        KillTimer(self.window, ID_SCRIPT_RUNTIME_TIMER);
        let advance = self.take_script_runtime_elapsed();
        let client = Arc::clone(&self.http_client);
        let mut additional_bytes = 0_u64;
        let mut additional_network_time = Duration::ZERO;
        let mut additional_processing_time = Duration::ZERO;
        let mut resource_budget = self.page_resource_budget;
        let script_started = Instant::now();
        let outcome = {
            let Some(runtime) = self.script_runtime.as_mut() else {
                self.script_runtime_clock = None;
                return;
            };
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
                additional_bytes += size;
                resource_budget -= size;
                Ok(code)
            };
            runtime.advance_time_with_loader(
                advance,
                MAX_POST_LOAD_TIMER_CALLBACKS,
                Some(&mut dynamic_script_loader),
            )
        };
        let script_time = script_started
            .elapsed()
            .saturating_sub(additional_network_time);

        self.complete_post_load_script_task(
            outcome,
            PostLoadScriptWork {
                script_time,
                network_time: additional_network_time,
                processing_time: additional_processing_time,
                bytes: additional_bytes,
                resource_budget,
            },
        );
    }

    pub(super) fn take_script_runtime_elapsed(&mut self) -> Duration {
        let now = Instant::now();
        self.script_runtime_clock
            .replace(now)
            .map(|previous| now.saturating_duration_since(previous))
            .unwrap_or_default()
    }

    pub(super) unsafe fn complete_post_load_script_task(
        &mut self,
        mut outcome: ScriptOutcome,
        mut work: PostLoadScriptWork,
    ) {
        let client = Arc::clone(&self.http_client);

        for cookie in &outcome.cookie_updates {
            if let Err(error) = client.set_cookie(&self.page.source_url, cookie) {
                outcome.errors.push(format!("document.cookie: {error}"));
            }
        }

        let mut style_refresh_time = Duration::ZERO;
        if outcome.render_requested && !outcome.runtime_stopped {
            let style_refresh_started = Instant::now();
            self.page
                .refresh_resources(self.current_style_viewport_width());
            style_refresh_time += style_refresh_started.elapsed();
            load_page_resources(
                &client,
                &mut self.page,
                &mut self.loaded_page_resources,
                &mut work.resource_budget,
                &mut work.bytes,
                &mut work.network_time,
                &mut work.processing_time,
            );
            self.web_fonts.clear();
            self.web_fonts.register(&self.page.fonts);
            self.dynamic_fonts.clear();
            self.page.title = self.page.dom.title();
            set_window_text(
                self.window,
                &format!("{} \u{2014} {PRODUCT_NAME}", self.page.title),
            );
            self.rebuild_layout();
            InvalidateRect(self.window, null(), 0);
        }

        self.page_resource_budget = work.resource_budget;
        self.record_post_load_script_outcome(
            &outcome,
            work.script_time,
            style_refresh_time,
            work.network_time,
            work.processing_time,
            work.bytes,
        );

        if outcome.runtime_stopped {
            self.cancel_script_runtime();
            return;
        }
        if let Some(navigation_url) = outcome.navigation_url
            && navigation_url != self.page.source_url
            && self.allow_script_navigation(&navigation_url)
        {
            self.begin_navigation(navigation_url, HistoryMode::Script);
            return;
        }
        if outcome.render_requested {
            let deferred_resources = self.unloaded_font_resources();
            self.begin_deferred_resources(deferred_resources);
        }
        self.schedule_script_runtime_wakeup();
    }

    pub(super) unsafe fn allow_script_navigation(&mut self, target: &str) -> bool {
        match self.script_navigation.allow(target) {
            Ok(()) => true,
            Err(error) => {
                self.set_status(&format!("Script navigation blocked: {error}"));
                false
            }
        }
    }
}

fn win32_timer_delay_ms(delay: Duration) -> u32 {
    let rounded_up = delay
        .as_millis()
        .saturating_add(u128::from(!delay.subsec_nanos().is_multiple_of(1_000_000)));
    rounded_up.clamp(USER_TIMER_MINIMUM_MS, USER_TIMER_MAXIMUM_MS) as u32
}

#[cfg(test)]
mod tests;
