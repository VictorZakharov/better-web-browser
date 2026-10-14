//! Preserve future deadlines when narrowing durations to the IPC clock unit.
use std::time::Duration;

pub(super) fn wake_micros(delay: Duration) -> u64 {
    // Truncation can turn a future opportunity into an early wake that still
    // has no runnable callback. Zero remains immediate and overflow saturates.
    delay.as_nanos().div_ceil(1_000).min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_timer_deadlines_round_up_and_preserve_immediate_work() {
        assert_eq!(wake_micros(Duration::ZERO), 0);
        assert_eq!(wake_micros(Duration::from_nanos(1)), 1);
        assert_eq!(wake_micros(Duration::from_nanos(2_083_334)), 2_084);
        assert_eq!(wake_micros(Duration::from_micros(10_000)), 10_000);
        assert_eq!(wake_micros(Duration::MAX), u64::MAX);
    }
}
