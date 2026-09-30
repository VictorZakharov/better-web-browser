use super::*;
use std::cell::RefCell;

struct NativeTimer {
    started: Instant,
    now_ms: u64,
    deadline_ms: Option<u64>,
    install_calls: Vec<(u64, u32)>,
    stop_calls: usize,
    fail_next_install: bool,
}

impl Default for NativeTimer {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            now_ms: 0,
            deadline_ms: None,
            install_calls: Vec::new(),
            stop_calls: 0,
            fail_next_install: false,
        }
    }
}

impl NativeTimer {
    fn install(&mut self, interval: u32) -> bool {
        self.install_calls.push((self.now_ms, interval));
        if std::mem::take(&mut self.fail_next_install) {
            return false;
        }
        self.deadline_ms = Some(self.now_ms + u64::from(interval));
        true
    }

    fn stop(&mut self) {
        self.stop_calls += 1;
        self.deadline_ms = None;
    }
}

fn request(
    monitor: &mut RendererMonitor,
    timer: &RefCell<NativeTimer>,
    interval: Option<u32>,
) -> bool {
    let now = {
        let timer = timer.borrow();
        timer.started + Duration::from_millis(timer.now_ms)
    };
    monitor.update(
        interval,
        now,
        |interval| timer.borrow_mut().install(interval),
        || timer.borrow_mut().stop(),
    )
}

#[test]
fn repeated_fast_input_requests_preserve_the_due_monitor_turn() {
    for cadence in [8, 10] {
        let mut monitor = RendererMonitor::default();
        let timer = RefCell::new(NativeTimer::default());
        assert!(request(
            &mut monitor,
            &timer,
            Some(ACTIVE_RENDERER_MONITOR_INTERVAL_MS)
        ));
        for input in 1..=32 {
            timer.borrow_mut().now_ms = input * cadence;
            assert!(request(
                &mut monitor,
                &timer,
                Some(ACTIVE_RENDERER_MONITOR_INTERVAL_MS)
            ));
            assert_eq!(timer.borrow().deadline_ms, Some(16));
        }
        assert_eq!(timer.borrow().install_calls, [(0, 16)]);
        assert!(timer.borrow().deadline_ms.unwrap() < timer.borrow().now_ms);
        assert_eq!(timer.borrow().stop_calls, 0);
        let started = timer.borrow().started;
        assert!(!monitor.service_due(started + Duration::from_millis(15)));
        assert!(monitor.service_due(started + Duration::from_millis(16)));
        assert_eq!(monitor.last_service, Some(started));
    }
}

#[test]
fn intentional_active_and_idle_rate_changes_rearm_once() {
    let mut monitor = RendererMonitor::default();
    let timer = RefCell::new(NativeTimer::default());
    assert!(request(
        &mut monitor,
        &timer,
        Some(RENDERER_MONITOR_INTERVAL_MS)
    ));
    assert_eq!(timer.borrow().deadline_ms, Some(250));
    timer.borrow_mut().now_ms = 8;
    assert!(request(
        &mut monitor,
        &timer,
        Some(ACTIVE_RENDERER_MONITOR_INTERVAL_MS)
    ));
    assert_eq!(timer.borrow().deadline_ms, Some(24));
    timer.borrow_mut().now_ms = 10;
    assert!(request(
        &mut monitor,
        &timer,
        Some(ACTIVE_RENDERER_MONITOR_INTERVAL_MS)
    ));
    assert_eq!(timer.borrow().deadline_ms, Some(24));
    timer.borrow_mut().now_ms = 40;
    assert!(request(
        &mut monitor,
        &timer,
        Some(RENDERER_MONITOR_INTERVAL_MS)
    ));
    assert_eq!(timer.borrow().deadline_ms, Some(290));
    assert!(request(
        &mut monitor,
        &timer,
        Some(RENDERER_MONITOR_INTERVAL_MS)
    ));
    assert_eq!(timer.borrow().install_calls, [(0, 250), (8, 16), (40, 250)]);
}

#[test]
fn no_live_renderer_stops_and_clears_before_a_fresh_restart() {
    let mut monitor = RendererMonitor::default();
    let timer = RefCell::new(NativeTimer::default());
    assert!(request(&mut monitor, &timer, Some(16)));
    assert!(request(&mut monitor, &timer, None));
    assert_eq!(monitor.installed_interval, None);
    assert_eq!(monitor.last_service, None);
    assert_eq!(timer.borrow().deadline_ms, None);
    assert_eq!(timer.borrow().stop_calls, 1);
    assert!(request(&mut monitor, &timer, None));
    assert_eq!(timer.borrow().stop_calls, 1);
    timer.borrow_mut().now_ms = 100;
    assert!(request(&mut monitor, &timer, Some(16)));
    assert_eq!(monitor.installed_interval, Some(16));
    assert_eq!(
        monitor.last_service,
        Some(timer.borrow().started + Duration::from_millis(100))
    );
    assert_eq!(timer.borrow().deadline_ms, Some(116));
    assert_eq!(timer.borrow().install_calls, [(0, 16), (100, 16)]);
}

#[test]
fn failed_initial_and_rate_change_installs_retry_until_success() {
    let mut monitor = RendererMonitor::default();
    let timer = RefCell::new(NativeTimer {
        fail_next_install: true,
        ..NativeTimer::default()
    });
    assert!(!request(&mut monitor, &timer, Some(16)));
    assert_eq!(monitor.installed_interval, None);
    assert_eq!(monitor.last_service, None);
    assert_eq!(timer.borrow().deadline_ms, None);
    timer.borrow_mut().now_ms = 10;
    assert!(request(&mut monitor, &timer, Some(16)));
    assert_eq!(monitor.installed_interval, Some(16));
    assert_eq!(timer.borrow().deadline_ms, Some(26));
    timer.borrow_mut().fail_next_install = true;
    let previous_service = monitor.last_service;
    assert!(!request(&mut monitor, &timer, Some(250)));
    assert_eq!(monitor.installed_interval, Some(16));
    assert_eq!(monitor.last_service, previous_service);
    assert_eq!(timer.borrow().deadline_ms, Some(26));
    timer.borrow_mut().now_ms = 20;
    assert!(request(&mut monitor, &timer, Some(250)));
    assert_eq!(monitor.installed_interval, Some(250));
    assert_eq!(timer.borrow().deadline_ms, Some(270));
    assert_eq!(
        timer.borrow().install_calls,
        [(0, 16), (10, 16), (10, 250), (20, 250)]
    );
}

#[test]
fn due_fallback_waits_for_the_installed_interval_and_marks_completed_work() {
    let mut monitor = RendererMonitor::default();
    let timer = RefCell::new(NativeTimer::default());
    let started = timer.borrow().started;
    assert!(!monitor.service_due(started + Duration::from_secs(1)));
    assert!(request(&mut monitor, &timer, Some(16)));
    assert!(!monitor.service_due(started + Duration::from_millis(15)));
    assert!(monitor.service_due(started + Duration::from_millis(16)));
    // A poll admitted at16ms finishes at40ms; time spent in its atomic report
    // is not permission to immediately start another whole-window turn.
    let completed = started + Duration::from_millis(40);
    monitor.serviced(completed);
    assert!(!monitor.service_due(completed + Duration::from_millis(15)));
    assert!(monitor.service_due(completed + Duration::from_millis(16)));
    assert!(!monitor.native_turn_ready(completed));
    assert!(!monitor.native_turn_ready(completed + Duration::from_micros(999)));
    assert!(monitor.native_turn_ready(completed + Duration::from_millis(1)));
    assert!(monitor.native_turn_ready(completed + Duration::from_millis(13)));
}

#[test]
fn successful_rate_changes_reset_due_time_but_failed_changes_preserve_it() {
    let mut monitor = RendererMonitor::default();
    let timer = RefCell::new(NativeTimer::default());
    let started = timer.borrow().started;
    assert!(request(&mut monitor, &timer, Some(16)));
    timer.borrow_mut().now_ms = 8;
    timer.borrow_mut().fail_next_install = true;
    assert!(!request(&mut monitor, &timer, Some(250)));
    assert!(monitor.service_due(started + Duration::from_millis(16)));
    timer.borrow_mut().now_ms = 10;
    assert!(request(&mut monitor, &timer, Some(250)));
    assert!(!monitor.service_due(started + Duration::from_millis(259)));
    assert!(monitor.service_due(started + Duration::from_millis(260)));
    assert!(request(&mut monitor, &timer, None));
    monitor.serviced(started + Duration::from_secs(1));
    assert_eq!(monitor.last_service, None);
    assert!(!monitor.service_due(started + Duration::from_secs(2)));
    timer.borrow_mut().now_ms = 1000;
    assert!(request(&mut monitor, &timer, Some(16)));
    assert!(!monitor.service_due(started + Duration::from_millis(1015)));
    assert!(monitor.service_due(started + Duration::from_millis(1016)));
}
