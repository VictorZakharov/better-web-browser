//! Page-ready scrolling trace for diagnosing UI-thread responsiveness.

mod summary;

use super::super::*;
use std::fmt::Write;
use summary::TraceSummary;

const TRACE_INTERVAL: Duration = Duration::from_millis(16);
const COMPLETION_GRACE: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Default)]
pub(in crate::windows_app) struct BenchmarkActivity {
    pub(in crate::windows_app) style_time: Duration,
    pub(in crate::windows_app) layout_time: Duration,
    pub(in crate::windows_app) paint_time: Duration,
    pub(in crate::windows_app) resource_time: Duration,
    pub(in crate::windows_app) script_time: Duration,
    pub(in crate::windows_app) layout_rebuilds: usize,
    pub(in crate::windows_app) display_items_invalidated: usize,
    pub(in crate::windows_app) paint_count: usize,
    pub(in crate::windows_app) resource_completions: usize,
    pub(in crate::windows_app) script_tasks: usize,
    pub(in crate::windows_app) invalidated_nodes: usize,
    pub(in crate::windows_app) full_style_rebuilds: usize,
}

impl BenchmarkActivity {
    fn since(self, previous: Self) -> ActivityDelta {
        ActivityDelta {
            style_time: self.style_time.saturating_sub(previous.style_time),
            layout_time: self.layout_time.saturating_sub(previous.layout_time),
            paint_time: self.paint_time.saturating_sub(previous.paint_time),
            resource_time: self.resource_time.saturating_sub(previous.resource_time),
            script_time: self.script_time.saturating_sub(previous.script_time),
            layout_rebuilds: self
                .layout_rebuilds
                .saturating_sub(previous.layout_rebuilds),
            display_items_invalidated: self
                .display_items_invalidated
                .saturating_sub(previous.display_items_invalidated),
            paint_count: self.paint_count.saturating_sub(previous.paint_count),
            resource_completions: self
                .resource_completions
                .saturating_sub(previous.resource_completions),
            script_tasks: self.script_tasks.saturating_sub(previous.script_tasks),
            invalidated_nodes: self
                .invalidated_nodes
                .saturating_sub(previous.invalidated_nodes),
            full_style_rebuilds: self
                .full_style_rebuilds
                .saturating_sub(previous.full_style_rebuilds),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ActivityDelta {
    style_time: Duration,
    layout_time: Duration,
    paint_time: Duration,
    resource_time: Duration,
    script_time: Duration,
    layout_rebuilds: usize,
    display_items_invalidated: usize,
    paint_count: usize,
    resource_completions: usize,
    script_tasks: usize,
    invalidated_nodes: usize,
    full_style_rebuilds: usize,
}

struct EarlyScrollSample {
    scheduled: Duration,
    handled: Duration,
    input_to_paint: Duration,
    frame_work: Duration,
    scroll_y: i32,
    activity: ActivityDelta,
}

pub(in crate::windows_app) struct EarlyScrollTrace {
    duration: Duration,
    started: Option<Instant>,
    previous_activity: BenchmarkActivity,
    samples: Vec<EarlyScrollSample>,
    direction: i32,
}

impl EarlyScrollTrace {
    pub(super) fn six_seconds() -> Self {
        Self {
            duration: Duration::from_secs(6),
            started: None,
            previous_activity: BenchmarkActivity::default(),
            samples: Vec::new(),
            direction: 1,
        }
    }

    pub(super) fn schedule(&mut self, window: Hwnd, settle: Duration, activity: BenchmarkActivity) {
        let started = Instant::now();
        self.started = Some(started);
        self.previous_activity = activity;
        let duration = self.duration;
        let window = window as usize;
        std::thread::spawn(move || {
            let sample_count = (duration.as_millis() / TRACE_INTERVAL.as_millis()).max(1) as usize;
            for sequence in 1..=sample_count {
                let deadline = started + TRACE_INTERVAL * sequence as u32;
                if let Some(delay) = deadline.checked_duration_since(Instant::now()) {
                    std::thread::sleep(delay);
                }
                if unsafe { PostMessageW(window as Hwnd, WM_APP_EARLY_SCROLL_TICK, sequence, 0) }
                    == 0
                {
                    return;
                }
            }
            // Leave a short tail after sampling so work deferred during continuous scrolling can
            // resume and remain represented in the benchmark report.
            let finish_at = started + settle.max(duration + COMPLETION_GRACE);
            if let Some(delay) = finish_at.checked_duration_since(Instant::now()) {
                std::thread::sleep(delay);
            }
            unsafe {
                PostMessageW(window as Hwnd, WM_APP_BENCHMARK_FINISH, 0, 0);
            }
        });
    }

    fn target_scroll(&mut self, current: i32, maximum: i32, step: i32) -> i32 {
        if maximum <= 0 {
            return 0;
        }
        let next = current.saturating_add(step.saturating_mul(self.direction));
        if next >= maximum {
            self.direction = -1;
            maximum
        } else if next <= 0 {
            self.direction = 1;
            0
        } else {
            next
        }
    }

    fn record(
        &mut self,
        sequence: usize,
        handled_at: Instant,
        painted_at: Instant,
        scroll_y: i32,
        activity: BenchmarkActivity,
    ) {
        let Some(started) = self.started else {
            return;
        };
        let scheduled = TRACE_INTERVAL * sequence as u32;
        let handled = handled_at.saturating_duration_since(started);
        let expected = started + scheduled;
        let sample = EarlyScrollSample {
            scheduled,
            handled,
            input_to_paint: painted_at.saturating_duration_since(expected),
            frame_work: painted_at.saturating_duration_since(handled_at),
            scroll_y,
            activity: activity.since(self.previous_activity),
        };
        self.previous_activity = activity;
        self.samples.push(sample);
    }

    pub(super) fn to_json(&self) -> String {
        let summary = TraceSummary::from_samples(&self.samples);
        let mut json = String::new();
        let _ = write!(
            json,
            concat!(
                "{{\n",
                "    \"duration_ms\": {:.3},\n",
                "    \"interval_ms\": {:.3},\n",
                "    \"summary\": {},\n",
                "    \"samples\": ["
            ),
            milliseconds(self.duration),
            milliseconds(TRACE_INTERVAL),
            summary.to_json(),
        );
        for (index, sample) in self.samples.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            let activity = sample.activity;
            let _ = write!(
                json,
                concat!(
                    "\n      {{\"scheduled_ms\": {:.3}, \"handled_ms\": {:.3}, ",
                    "\"input_to_paint_ms\": {:.3}, \"frame_work_ms\": {:.3}, ",
                    "\"scroll_y\": {}, \"style_ms\": {:.3}, \"layout_ms\": {:.3}, ",
                    "\"display_items_invalidated\": {}, \"paint_ms\": {:.3}, ",
                    "\"paint_count\": {}, \"resource_ms\": {:.3}, ",
                    "\"resource_completions\": {}, \"script_ms\": {:.3}, ",
                    "\"script_tasks\": {}, \"invalidated_nodes\": {}, ",
                    "\"layout_rebuilds\": {}, \"full_style_rebuilds\": {}}}"
                ),
                milliseconds(sample.scheduled),
                milliseconds(sample.handled),
                milliseconds(sample.input_to_paint),
                milliseconds(sample.frame_work),
                sample.scroll_y,
                milliseconds(activity.style_time),
                milliseconds(activity.layout_time),
                activity.display_items_invalidated,
                milliseconds(activity.paint_time),
                activity.paint_count,
                milliseconds(activity.resource_time),
                activity.resource_completions,
                milliseconds(activity.script_time),
                activity.script_tasks,
                activity.invalidated_nodes,
                activity.layout_rebuilds,
                activity.full_style_rebuilds,
            );
        }
        json.push_str("\n    ]\n  }");
        json
    }
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

impl BrowserState {
    pub(in crate::windows_app) unsafe fn handle_early_scroll_tick(&mut self, sequence: usize) {
        let maximum = (self.content_height - self.viewport_height()).max(0);
        let step = self.scale(42);
        let current = self.scroll_y;
        let target = self
            .benchmark
            .as_mut()
            .and_then(|benchmark| benchmark.early_scroll.as_mut())
            .map(|trace| trace.target_scroll(current, maximum, step));
        let Some(target) = target else {
            return;
        };
        let handled_at = Instant::now();
        self.scroll_to(target);
        // scroll_to owns frame completion: UpdateWindow for interactive windows and a retained
        // display-list offscreen substitute when this window is deliberately hidden.
        let painted_at = Instant::now();
        let activity = self
            .benchmark
            .as_ref()
            .map(|benchmark| benchmark.activity)
            .unwrap_or_default();
        let scroll_y = self.scroll_y;
        if let Some(trace) = self
            .benchmark
            .as_mut()
            .and_then(|benchmark| benchmark.early_scroll.as_mut())
        {
            trace.record(sequence, handled_at, painted_at, scroll_y, activity);
        }
    }

    pub(in crate::windows_app) fn record_benchmark_paint(&mut self, duration: Duration) {
        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.activity.paint_time += duration;
            benchmark.activity.paint_count += 1;
        }
    }

    pub(in crate::windows_app) fn record_benchmark_layout(
        &mut self,
        duration: Duration,
        damage: DisplayListDamage,
    ) {
        self.record_performance_activity(PerformanceActivity::Layout, duration);
        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.activity.layout_time += duration;
            benchmark.activity.layout_rebuilds += 1;
            benchmark.activity.display_items_invalidated += damage.changed_items;
        }
    }

    pub(in crate::windows_app) fn record_benchmark_resource_completion(
        &mut self,
        duration: Duration,
        completions: usize,
    ) {
        self.record_performance_activity(PerformanceActivity::Resource, duration);
        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.activity.resource_time += duration;
            benchmark.activity.resource_completions += completions;
        }
    }
}
