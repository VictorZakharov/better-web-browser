//! One UI-thread-owned, nonperiodic high-resolution rendering wake.
use super::*;
use windows_sys::Win32::Foundation::{CloseHandle, WAIT_FAILED};
use windows_sys::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, INFINITE,
    SYNCHRONIZATION_SYNCHRONIZE, SetWaitableTimerEx, TIMER_MODIFY_STATE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, QS_ALLINPUT,
};

pub(super) struct DeadlineTimer(Handle);

impl DeadlineTimer {
    pub(super) fn new() -> Option<Self> {
        // Unnamed and non-inheritable; automatic reset consumes exactly one wake.
        // Unsupported systems retain the existing window timer fallback.
        // https://learn.microsoft.com/windows/win32/api/synchapi/nf-synchapi-createwaitabletimerexw
        let handle = unsafe {
            CreateWaitableTimerExW(
                null(),
                null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                SYNCHRONIZATION_SYNCHRONIZE | TIMER_MODIFY_STATE,
            )
        };
        if handle.is_null() {
            None
        } else {
            Some(Self(handle))
        }
    }

    pub(super) fn arm(&self, remaining: Duration) -> bool {
        let relative = relative_due_time(remaining);
        // Negative units are relative 100 ns intervals. Zero period, no APC,
        // no wake-from-sleep request and no tolerated coalescing delay.
        // https://learn.microsoft.com/windows/win32/api/synchapi/nf-synchapi-setwaitabletimerex
        unsafe { SetWaitableTimerEx(self.0, &relative, 0, None, null(), null(), 0) != 0 }
    }

    pub(super) fn wait_for_messages(&self) -> Result<(), String> {
        // Seen-but-not-removed input must also interrupt the wait. Sent messages
        // are dispatched by the next PeekMessage, with no retained state borrow.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-msgwaitformultipleobjectsex
        let result = unsafe {
            MsgWaitForMultipleObjectsEx(1, &self.0, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if result == WAIT_FAILED {
            Err(last_error("wait for renderer deadline or window message"))
        } else {
            Ok(())
        }
    }
}

impl Drop for DeadlineTimer {
    fn drop(&mut self) {
        // Only this UI thread can wait; run() has returned before handle closure.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn relative_due_time(remaining: Duration) -> i64 {
    // Ceiling, not truncation: never deliver a future logical task early.
    -i64::try_from(
        remaining
            .as_nanos()
            .div_ceil(100)
            .clamp(1, i64::MAX as u128),
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relative_timer_units_round_up_and_keep_overdue_and_huge_times_bounded() {
        assert_eq!(relative_due_time(Duration::ZERO), -1);
        assert_eq!(relative_due_time(Duration::from_nanos(1)), -1);
        assert_eq!(relative_due_time(Duration::from_nanos(100)), -1);
        assert_eq!(relative_due_time(Duration::from_nanos(101)), -2);
        assert_eq!(relative_due_time(Duration::from_millis(16)), -160_000);
        assert_eq!(relative_due_time(Duration::MAX), -i64::MAX);
    }

    #[test]
    fn native_deadline_is_one_shot_and_does_not_leave_a_periodic_wake() {
        use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        let Some(timer) = DeadlineTimer::new() else {
            return;
        };
        assert!(timer.arm(Duration::from_millis(1)));
        assert_eq!(unsafe { WaitForSingleObject(timer.0, 1000) }, WAIT_OBJECT_0);
        assert_eq!(unsafe { WaitForSingleObject(timer.0, 0) }, WAIT_TIMEOUT);
    }

    #[test]
    fn already_queued_messages_interrupt_the_native_deadline_wait() {
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
        std::thread::spawn(|| {
            let Some(timer) = DeadlineTimer::new() else {
                return;
            };
            unsafe {
                let mut message = std::mem::zeroed();
                // Create this thread's private queue without opening a window.
                PeekMessageW(&mut message, null_mut(), 0, 0, 0);
                assert_ne!(
                    PostThreadMessageW(GetCurrentThreadId(), WM_APP + 77, 23, 45),
                    0
                );
                assert!(timer.arm(Duration::from_secs(1)));
                timer.wait_for_messages().unwrap();
                assert_ne!(
                    PeekMessageW(
                        &mut message,
                        null_mut(),
                        WM_APP + 77,
                        WM_APP + 77,
                        PM_REMOVE
                    ),
                    0
                );
                assert_eq!((message.wparam, message.lparam), (23, 45));
            }
        })
        .join()
        .unwrap();
    }
}
