//! Win32 display power request; only constructed and mutated in the browser UI process.

use super::PlatformWakeLock;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Power::{
    PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired,
    PowerSetRequest,
};
use windows_sys::Win32::System::Threading::{
    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};

#[derive(Default)]
pub(in crate::windows_app) struct DisplayPowerRequest {
    handle: Option<HANDLE>,
}

impl PlatformWakeLock for DisplayPowerRequest {
    fn acquire(&mut self) -> bool {
        if self.handle.is_some() {
            return true;
        }
        let mut reason: Vec<u16> = "Breeze screen wake lock\0".encode_utf16().collect();
        let context = REASON_CONTEXT {
            Version: 0, // POWER_REQUEST_CONTEXT_VERSION in the Win32 SDK.
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: reason.as_mut_ptr(),
            },
        };
        let handle = unsafe { PowerCreateRequest(&context) };
        if handle.is_null() || handle == -1_isize as HANDLE {
            return false;
        }
        // Windows documents that display-required alone does not prevent sleep.
        let display = unsafe { PowerSetRequest(handle, PowerRequestDisplayRequired) } != 0;
        let system = display && unsafe { PowerSetRequest(handle, PowerRequestSystemRequired) } != 0;
        if display && system {
            self.handle = Some(handle);
            true
        } else {
            if display {
                unsafe {
                    PowerClearRequest(handle, PowerRequestDisplayRequired);
                }
            }
            unsafe {
                CloseHandle(handle);
            }
            false
        }
    }

    fn release(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        // Closing the handle also retires a request if PowerClearRequest fails.
        unsafe {
            PowerClearRequest(handle, PowerRequestSystemRequired);
            PowerClearRequest(handle, PowerRequestDisplayRequired);
            CloseHandle(handle);
        }
    }
}

impl Drop for DisplayPowerRequest {
    fn drop(&mut self) {
        self.release();
    }
}
