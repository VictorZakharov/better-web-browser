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
