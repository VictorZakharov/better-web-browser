//! UI-thread ownership and wakeups for a document's retained JavaScript realm.
//! Boa contexts remain on the thread that creates them. Win32 posts `WM_TIMER` to the owning
//! window's thread, so the document, realm, style invalidation, layout, and painting all stay on
//! one owner without cross-thread runtime transfer.
//! Platform references:
//! - <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-settimer>
//! - <https://learn.microsoft.com/windows/win32/winmsg/wm-timer>

use super::browser_navigation::HistoryMode;
use super::resources::{ResourceLoadContext, fetch_document_resource, load_page_resources};
use super::*;
use better_web_browser::fetch::RequestDestination;
const USER_TIMER_MINIMUM_MS: u128 = 10;
const USER_TIMER_MAXIMUM_MS: u128 = 0x7fff_ffff;
// HTML performs rendering and user-interaction steps between event-loop tasks. A Win32 timer
// wakeup therefore executes one JavaScript timer task instead of draining every overdue callback.
// <https://html.spec.whatwg.org/multipage/webappapis.html#event-loop-processing-model>
pub(super) const TIMER_CALLBACKS_PER_WAKEUP: usize = 1;
// Boa callbacks cannot be preempted safely, so keep low-priority post-load script tasks out of an
// active scroll gesture and resume promptly once input becomes quiet.
const SCROLL_QUIET_PERIOD: Duration = Duration::from_millis(100);
// First paint must be followed by a real interaction opportunity. Boa's synchronous embedding API
// cannot preempt a post-load task once it starts, so starting one in the same turn as first paint
// can make an otherwise-ready page ignore the user's first wheel input for hundreds of milliseconds.
const INITIAL_INTERACTION_GRACE: Duration = Duration::from_millis(750);

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
        self.post_load_script_not_before = None;
    }

    pub(super) unsafe fn install_script_runtime(&mut self, runtime: Option<ScriptRuntime>) {
        self.cancel_script_runtime();
        self.script_runtime = runtime;
        let installed = self.script_runtime.as_ref().map(|_| Instant::now());
        self.script_runtime_clock = installed;
        self.post_load_script_not_before = installed
            .filter(|_| self.benchmark.is_none())
            .map(|now| now + INITIAL_INTERACTION_GRACE);
        self.schedule_script_runtime_wakeup();
    }

    pub(super) unsafe fn resume_script_runtime(&mut self) {
        if self.script_runtime.is_some() {
            self.script_runtime_clock = Some(Instant::now());
            self.schedule_script_runtime_wakeup();
        }
    }

    pub(super) unsafe fn schedule_script_runtime_wakeup(&mut self) {
        KillTimer(self.window, ID_SCRIPT_RUNTIME_TIMER);
        let pending_async_script = self
            .tabs
            .iter()
            .any(|tab| !tab.pending_async_scripts.is_empty());
        let next_delay = if pending_async_script {
            Some(Duration::ZERO)
        } else {
            self.script_runtime
                .as_mut()
                .and_then(ScriptRuntime::next_timer_delay)
        };
        let Some(next_delay) = next_delay else {
            return;
        };
        let next_delay = self
            .remaining_scroll_quiet_period(Instant::now())
            .map_or(next_delay, |quiet| next_delay.max(quiet));
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
        if self.remaining_scroll_quiet_period(Instant::now()).is_some() {
            self.schedule_script_runtime_wakeup();
            return;
        }
        let pending_async_script = self.tabs.iter_mut().find_map(|tab| {
            let id = tab.id;
            tab.pending_async_scripts
                .pop_front()
                .map(|message| (id, message))
        });
        if let Some((id, message)) = pending_async_script {
            self.route_async_script_message(id, message);
            self.schedule_script_runtime_wakeup();
            return;
        }
        let advance = self.take_script_runtime_elapsed();
        let client = Arc::clone(&self.http_client);
        let fetch_signal = self.document_fetch.signal();
        let document_url = self.page.source_url.clone();
        let mut additional_bytes = 0_u64;
        let mut additional_network_time = Duration::ZERO;
        let mut additional_processing_time = Duration::ZERO;
        let mut resource_budget = self.page_resource_budget;
        let cookie_header = client.document_cookie_header(&document_url);
        let script_started = Instant::now();
        let mut outcome = {
            let Some(runtime) = self.script_runtime.as_mut() else {
                self.script_runtime_clock = None;
                return;
            };
            if let Ok(header) = &cookie_header {
                runtime.set_document_cookie_header(header);
            }
            let mut dynamic_script_loader = |url: &str| -> Result<String, String> {
                let request_started = Instant::now();
                let response = fetch_document_resource(
                    &client,
                    &fetch_signal,
                    &document_url,
                    url,
                    RequestDestination::Script,
                )
                .map_err(|error| error.to_string());
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
                let code = winhttp::decode_text(response.body.as_bytes(), response.content_type());
                additional_processing_time += processing_started.elapsed();
                additional_bytes += size;
                resource_budget -= size;
                Ok(code)
            };
            runtime.advance_time_with_loader(
                advance,
                TIMER_CALLBACKS_PER_WAKEUP,
                Some(&mut dynamic_script_loader),
            )
        };
        if let Err(error) = cookie_header {
            outcome
                .errors
                .push(format!("document.cookie refresh: {error}"));
        }
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

    pub(super) unsafe fn note_scroll_activity(&mut self) {
        self.last_scroll_activity = Some(Instant::now());
        self.schedule_script_runtime_wakeup();
    }

    pub(super) fn should_defer_script_work(&self) -> bool {
        self.remaining_scroll_quiet_period(Instant::now()).is_some()
    }

    fn remaining_scroll_quiet_period(&self, now: Instant) -> Option<Duration> {
        remaining_script_quiet_period(
            self.last_scroll_activity,
            self.post_load_script_not_before,
            now,
        )
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
        let mut render_metrics = None;
        if outcome.render_requested && !outcome.runtime_stopped {
            let style_refresh_started = Instant::now();
            let viewport_width = self.current_style_viewport_width();
            let fetch_signal = self.document_fetch.signal();
            let tab = self.tabs.active_mut();
            let style = tab
                .page
                .refresh_resources_after_invalidation(viewport_width, &outcome.invalidation);
            style_refresh_time += style_refresh_started.elapsed();
            load_page_resources(
                &mut tab.page,
                ResourceLoadContext {
                    client: &client,
                    signal: &fetch_signal,
                    loaded: &mut tab.loaded_page_resources,
                    resource_budget: &mut work.resource_budget,
                    bytes: &mut work.bytes,
                    network_time: &mut work.network_time,
                    processing_time: &mut work.processing_time,
                },
            );
            tab.web_fonts.clear();
            tab.web_fonts.register(&tab.page.fonts);
            tab.dynamic_fonts.clear();
            tab.page.title = tab.page.dom.title();
            let page_title = tab.page.title.clone();
            self.update_active_tab_title(&page_title);
            let damage = self.rebuild_layout();
            self.invalidate_layout_damage(damage);
            let metrics = super::runtime_metrics::RenderCheckpointMetrics { style, damage };
            outcome.diagnostics.push(metrics.diagnostic(&outcome));
            render_metrics = Some(metrics);
        }

        self.page_resource_budget = work.resource_budget;
        self.record_post_load_script_outcome(&outcome, &work, style_refresh_time, render_metrics);

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
            let deferred_resources = self.unloaded_deferred_resources();
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

fn remaining_quiet_period(
    last_activity: Option<Instant>,
    now: Instant,
    quiet_period: Duration,
) -> Option<Duration> {
    last_activity
        .and_then(|last| quiet_period.checked_sub(now.saturating_duration_since(last)))
        .filter(|remaining| !remaining.is_zero())
}

fn remaining_script_quiet_period(
    last_scroll_activity: Option<Instant>,
    post_load_not_before: Option<Instant>,
    now: Instant,
) -> Option<Duration> {
    let scroll = remaining_quiet_period(last_scroll_activity, now, SCROLL_QUIET_PERIOD);
    let initial = post_load_not_before
        .and_then(|deadline| deadline.checked_duration_since(now))
        .filter(|remaining| !remaining.is_zero());
    scroll.max(initial)
}

#[cfg(test)]
mod tests;
