use super::*;

#[test]
fn win32_wakeups_round_up_and_respect_platform_bounds() {
    assert_eq!(win32_timer_delay_ms(Duration::ZERO), 10);
    assert_eq!(win32_timer_delay_ms(Duration::from_nanos(10_000_001)), 11);
    assert_eq!(win32_timer_delay_ms(Duration::from_millis(250)), 250);
    assert_eq!(
        win32_timer_delay_ms(Duration::from_secs(u64::MAX)),
        0x7fff_ffff
    );
}

#[test]
fn each_windows_wakeup_runs_one_event_loop_task() {
    assert_eq!(TIMER_CALLBACKS_PER_WAKEUP, 1);
}

#[test]
fn script_work_waits_for_the_scroll_quiet_period() {
    let now = Instant::now();
    assert_eq!(
        remaining_quiet_period(
            Some(now - Duration::from_millis(25)),
            now,
            Duration::from_millis(100)
        ),
        Some(Duration::from_millis(75))
    );
    assert_eq!(
        remaining_quiet_period(
            Some(now - Duration::from_millis(100)),
            now,
            Duration::from_millis(100)
        ),
        None
    );
}

#[test]
fn later_quiet_deadline_wins() {
    let now = Instant::now();
    let remaining = remaining_script_quiet_period(
        Some(now - Duration::from_millis(50)),
        Some(now + Duration::from_millis(400)),
        now,
    );
    assert_eq!(remaining, Some(Duration::from_millis(400)));
}
