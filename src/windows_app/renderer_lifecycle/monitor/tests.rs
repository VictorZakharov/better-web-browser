use super::*;
use std::cell::RefCell;

#[derive(Default)]
struct NativeTimer {
    now_ms: u64,
    deadline_ms: Option<u64>,
    install_calls: Vec<(u64, u32)>,
    stop_calls: usize,
    fail_next_install: bool,
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
    monitor.update(
        interval,
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
    assert_eq!(timer.borrow().deadline_ms, None);
    assert_eq!(timer.borrow().stop_calls, 1);
    assert!(request(&mut monitor, &timer, None));
    assert_eq!(timer.borrow().stop_calls, 1);
    timer.borrow_mut().now_ms = 100;
    assert!(request(&mut monitor, &timer, Some(16)));
    assert_eq!(monitor.installed_interval, Some(16));
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
    assert_eq!(timer.borrow().deadline_ms, None);
    timer.borrow_mut().now_ms = 10;
    assert!(request(&mut monitor, &timer, Some(16)));
    assert_eq!(monitor.installed_interval, Some(16));
    assert_eq!(timer.borrow().deadline_ms, Some(26));
    timer.borrow_mut().fail_next_install = true;
    assert!(!request(&mut monitor, &timer, Some(250)));
    assert_eq!(monitor.installed_interval, Some(16));
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
