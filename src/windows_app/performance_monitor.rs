//! Per-tab, rolling UI responsiveness metrics and their lightweight in-browser monitor.

mod paint;
mod window;

pub(super) use window::window_proc;
pub(super) const CLASS_NAME: &str = "BetterWebBrowserPerformanceWindow";

use super::*;
use std::collections::VecDeque;

const FRAME_WINDOW: Duration = Duration::from_secs(2);
const FPS_WINDOW: Duration = Duration::from_secs(1);
const FPS_IDLE_AFTER: Duration = Duration::from_millis(350);
const LAST_SCROLL_FPS_VISIBLE_FOR: Duration = Duration::from_secs(30);
const MAX_FRAME_SAMPLES: usize = 240;
const MAX_ACTIVITY_SAMPLES: usize = 512;

#[derive(Clone, Copy)]
struct TimedDuration {
    completed: Instant,
    duration: Duration,
}

#[derive(Clone, Copy)]
pub(super) enum PerformanceActivity {
    Script,
    Style,
    Layout,
    Resource,
}

#[derive(Clone, Copy)]
struct ActivitySample {
    completed: Instant,
    duration: Duration,
    kind: PerformanceActivity,
}

#[derive(Clone, Copy)]
struct CompletedFrameSequence {
    completed: Instant,
    fps: f64,
}

#[derive(Default)]
pub(super) struct TabPerformance {
    frames: VecDeque<TimedDuration>,
    activity: VecDeque<ActivitySample>,
    frame_sequence_started: Option<Instant>,
    frame_sequence_completed: Option<Instant>,
    last_frame_sequence: Option<CompletedFrameSequence>,
}

#[derive(Default)]
struct PerformanceSnapshot {
    fps: Option<f64>,
    last_scroll_fps: Option<f64>,
    frame_p95: Duration,
    frame_maximum: Duration,
    long_frames: usize,
    paint_time: Duration,
    script_time: Duration,
    style_time: Duration,
    layout_time: Duration,
    resource_time: Duration,
    frame_history: Vec<Duration>,
}

impl TabPerformance {
    pub(in crate::windows_app) fn begin_frame_sequence(&mut self, started: Instant) {
        self.frame_sequence_started = Some(started);
        self.frame_sequence_completed = None;
    }

    pub(in crate::windows_app) fn end_frame_sequence(&mut self, completed: Instant) {
        if let Some(started) = self.frame_sequence_started {
            self.frame_sequence_completed = Some(completed);
            if let Some(fps) = frames_per_second(&self.frames, started, completed) {
                self.last_frame_sequence = Some(CompletedFrameSequence { completed, fps });
            }
        }
    }

    fn record_frame(&mut self, completed: Instant, duration: Duration) {
        self.frames.push_back(TimedDuration {
            completed,
            duration,
        });
        prune_timed(&mut self.frames, completed, FRAME_WINDOW, MAX_FRAME_SAMPLES);
    }

    fn record_activity(
        &mut self,
        completed: Instant,
        kind: PerformanceActivity,
        duration: Duration,
    ) {
        if duration.is_zero() {
            return;
        }
        self.activity.push_back(ActivitySample {
            completed,
            duration,
            kind,
        });
        while self.activity.len() > MAX_ACTIVITY_SAMPLES
            || self
                .activity
                .front()
                .is_some_and(|sample| completed.duration_since(sample.completed) > FRAME_WINDOW)
        {
            self.activity.pop_front();
        }
    }

    fn snapshot(&self, now: Instant) -> PerformanceSnapshot {
        let cutoff = now.checked_sub(FRAME_WINDOW).unwrap_or(now);
        let frames = self
            .frames
            .iter()
            .filter(|sample| sample.completed >= cutoff)
            .copied()
            .collect::<Vec<_>>();
        let fps = frames
            .last()
            .filter(|last| now.duration_since(last.completed) <= FPS_IDLE_AFTER)
            .and_then(|_| {
                let fps_cutoff = now.checked_sub(FPS_WINDOW).unwrap_or(now);
                let sequence_start = self.frame_sequence_started?.max(fps_cutoff);
                let sequence_end = self.frame_sequence_completed.unwrap_or(now);
                frames_per_second(&frames, sequence_start, sequence_end)
            });
        let last_scroll_fps = self
            .last_frame_sequence
            .filter(|sequence| {
                now.saturating_duration_since(sequence.completed) <= LAST_SCROLL_FPS_VISIBLE_FOR
            })
            .map(|sequence| sequence.fps);
        let paint_durations = frames
            .iter()
            .map(|sample| sample.duration)
            .collect::<Vec<_>>();
        let mut frame_intervals = frames
            .windows(2)
            .map(|pair| pair[1].completed.duration_since(pair[0].completed))
            .collect::<Vec<_>>();
        let frame_history = frame_intervals
            .iter()
            .rev()
            .take(60)
            .rev()
            .copied()
            .collect();
        frame_intervals.sort_unstable();
        let frame_p95 = percentile(&frame_intervals, 0.95);
        let frame_maximum = frame_intervals.last().copied().unwrap_or_default();
        let long_frames = frame_intervals
            .iter()
            .filter(|duration| **duration > Duration::from_micros(33_333))
            .count();
        let paint_time = paint_durations.iter().copied().sum();

        let mut snapshot = PerformanceSnapshot {
            fps,
            last_scroll_fps,
            frame_p95,
            frame_maximum,
            long_frames,
            paint_time,
            frame_history,
            ..PerformanceSnapshot::default()
        };
        for sample in self
            .activity
            .iter()
            .filter(|sample| sample.completed >= cutoff)
        {
            match sample.kind {
                PerformanceActivity::Script => snapshot.script_time += sample.duration,
                PerformanceActivity::Style => snapshot.style_time += sample.duration,
                PerformanceActivity::Layout => snapshot.layout_time += sample.duration,
                PerformanceActivity::Resource => snapshot.resource_time += sample.duration,
            }
        }
        snapshot
    }
}

fn frames_per_second<'a>(
    frames: impl IntoIterator<Item = &'a TimedDuration>,
    started: Instant,
    completed: Instant,
) -> Option<f64> {
    let mut matching = frames
        .into_iter()
        .filter(|sample| sample.completed >= started && sample.completed <= completed);
    let first = matching.next()?;
    let mut last = first;
    let mut intervals = 0_usize;
    for sample in matching {
        last = sample;
        intervals += 1;
    }
    let elapsed = last.completed.saturating_duration_since(first.completed);
    (intervals > 0 && !elapsed.is_zero()).then(|| intervals as f64 / elapsed.as_secs_f64())
}

impl BrowserState {
    pub(super) fn record_visible_paint(&mut self, duration: Duration) {
        if self.benchmark.is_none() {
            self.tabs
                .active_mut()
                .performance
                .record_frame(Instant::now(), duration);
        }
    }

    pub(super) fn record_performance_activity(
        &mut self,
        kind: PerformanceActivity,
        duration: Duration,
    ) {
        self.tabs
            .active_mut()
            .performance
            .record_activity(Instant::now(), kind, duration);
    }

    pub(super) fn performance_counter_rect(&self) -> Rect {
        Rect {
            left: (self.chrome.status.right - self.scale(96)).max(self.chrome.status.left),
            top: self.chrome.status.top,
            right: self.chrome.status.right,
            bottom: self.chrome.status.bottom,
        }
    }

    pub(super) unsafe fn toggle_performance_at(&mut self, x: i32, y: i32) -> bool {
        let bounds = self.performance_counter_rect();
        if x < bounds.left || x >= bounds.right || y < bounds.top || y >= bounds.bottom {
            return false;
        }
        self.toggle_performance_panel();
        true
    }

    pub(super) unsafe fn toggle_performance_panel(&mut self) {
        self.performance_panel_visible = !self.performance_panel_visible;
        self.position_performance_window();
        if !self.performance_window.is_null() {
            ShowWindow(
                self.performance_window,
                if self.performance_panel_visible {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
            InvalidateRect(self.performance_window, null(), 0);
        }
        let bounds = self.performance_counter_rect();
        InvalidateRect(self.window, &bounds, 0);
    }

    pub(super) unsafe fn refresh_performance_monitor(&mut self) {
        let counter = self.performance_counter_rect();
        InvalidateRect(self.window, &counter, 0);
        if self.performance_panel_visible && !self.performance_window.is_null() {
            self.position_performance_window();
            InvalidateRect(self.performance_window, null(), 0);
        }
    }
}

fn prune_timed(
    samples: &mut VecDeque<TimedDuration>,
    now: Instant,
    window: Duration,
    maximum: usize,
) {
    while samples.len() > maximum
        || samples
            .front()
            .is_some_and(|sample| now.duration_since(sample.completed) > window)
    {
        samples.pop_front();
    }
}

fn percentile(sorted: &[Duration], percentile: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reports_recent_frames_and_ignores_idle_history() {
        let start = Instant::now();
        let mut performance = TabPerformance::default();
        performance.begin_frame_sequence(start);
        for index in 0..=60 {
            performance.record_frame(
                start + Duration::from_millis(index * 16),
                Duration::from_millis(4),
            );
        }
        performance.end_frame_sequence(start + Duration::from_millis(960));
        let active = performance.snapshot(start + Duration::from_millis(960));
        assert!(active.fps.is_some_and(|fps| (62.0..=63.0).contains(&fps)));
        assert_eq!(active.frame_p95, Duration::from_millis(16));

        let idle = performance.snapshot(start + Duration::from_secs(2));
        assert_eq!(idle.fps, None);
        assert!(
            idle.last_scroll_fps
                .is_some_and(|fps| (62.0..=63.0).contains(&fps))
        );
    }

    #[test]
    fn fps_uses_the_current_animation_instead_of_an_older_idle_gap() {
        let start = Instant::now();
        let mut performance = TabPerformance::default();
        performance.record_frame(start, Duration::from_millis(4));
        let animation = start + Duration::from_millis(500);
        performance.begin_frame_sequence(animation);
        for index in 0..=12 {
            performance.record_frame(
                animation + Duration::from_millis(index * 16),
                Duration::from_millis(3),
            );
        }
        performance.end_frame_sequence(animation + Duration::from_millis(192));
        let snapshot = performance.snapshot(animation + Duration::from_millis(200));
        assert!(snapshot.fps.is_some_and(|fps| (62.0..=63.0).contains(&fps)));
        assert!(
            snapshot
                .last_scroll_fps
                .is_some_and(|fps| (62.0..=63.0).contains(&fps))
        );
        assert_eq!(snapshot.frame_maximum, Duration::from_millis(500));
    }

    #[test]
    fn completed_scroll_fps_expires_instead_of_following_a_tab_forever() {
        let start = Instant::now();
        let mut performance = TabPerformance::default();
        performance.begin_frame_sequence(start);
        for index in 0..=12 {
            performance.record_frame(
                start + Duration::from_millis(index * 16),
                Duration::from_millis(3),
            );
        }
        performance.end_frame_sequence(start + Duration::from_millis(192));

        let expired =
            performance.snapshot(start + LAST_SCROLL_FPS_VISIBLE_FOR + Duration::from_secs(1));
        assert_eq!(expired.last_scroll_fps, None);
    }
}
