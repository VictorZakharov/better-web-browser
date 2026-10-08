//! Owner-thread CPU attribution distinguishes execution from scheduler delay.
//! Diagnostic only: neither CPU accounting nor its failure changes a deadline.
use std::time::Duration;

#[cfg(windows)]
pub(in crate::engine::script) fn sample() -> Option<Duration> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: the pseudo-handle identifies this thread, is not retained/closed,
    // and all four output pointers refer to live, distinct FILETIME values.
    let success = unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if success == 0 {
        return None;
    }
    let ticks =
        |time: FILETIME| u64::from(time.dwLowDateTime) | (u64::from(time.dwHighDateTime) << 32);
    // FILETIME CPU totals use 100 ns units. Saturation is diagnostic-only and
    // prevents arithmetic overflow from affecting task delivery.
    Some(Duration::from_nanos(
        ticks(kernel)
            .saturating_add(ticks(user))
            .saturating_mul(100),
    ))
}

#[cfg(not(windows))]
pub(in crate::engine::script) fn sample() -> Option<Duration> {
    None
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn cpu_samples_are_monotonic_on_the_owner_thread() {
        let before = sample().expect("current-thread CPU time is available");
        let mut sum = 0_u64;
        for index in 0..100_000 {
            sum = std::hint::black_box(sum.wrapping_add(index));
        }
        std::hint::black_box(sum);
        let after = sample().unwrap();
        // Accounting has OS-dependent resolution; small work may round to zero.
        assert!(after >= before);
    }
}
