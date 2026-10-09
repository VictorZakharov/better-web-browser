//! Match V8's checked JS/Wasm boundary to the actual owner-thread stack.
//! An OS-stack overflow is a process failure, not a catchable JS RangeError.
use super::value::JsResult;
#[cfg(windows)]
use super::value::{JsError, JsErrorKind};

#[cfg(any(windows, test))]
mod policy;

pub(super) fn create_params() -> JsResult<v8::CreateParams> {
    let params = v8::CreateParams::default();
    #[cfg(windows)]
    {
        let boundary = current_boundary()?;
        // SAFETY: boundary is inside this thread's OS-reserved stack and leaves
        // native headroom. It is an address threshold, never a pointer to marker.
        // Context/Agent are Rc-owned, !Send, and disposed before this thread exits.
        // No fibers, isolate migration, or stack switching are supported.
        // https://chromium.googlesource.com/v8/v8/+/refs/heads/main/include/v8-isolate.h
        Ok(unsafe { params.set_stack_limit(std::ptr::with_exposed_provenance_mut(boundary)) })
    }
    #[cfg(not(windows))]
    Ok(params)
}

pub(super) struct Boundary {
    #[cfg(windows)]
    limit: usize,
}

impl Boundary {
    pub(super) fn new() -> JsResult<Self> {
        Ok(Self {
            #[cfg(windows)]
            limit: current_boundary()?,
        })
    }

    /// Creation constraints alone are not the active thread's entry guard.
    /// Reapply after Enter, before any JS/Wasm or context initialization.
    /// Never lower the lifetime threshold when re-entering from a deeper frame.
    /// Public SetStackLimit preserves a pending interrupt/termination sentinel.
    pub(super) fn install_for_entered_isolate(&self) -> JsResult<()> {
        #[cfg(windows)]
        if !super::v8_api::set_stack_limit(self.limit.max(current_boundary()?)) {
            return Err(JsError {
                kind: JsErrorKind::Error,
                message: "could not install the JavaScript owner-thread stack boundary".into(),
            });
        }
        Ok(())
    }
}

#[cfg(windows)]
fn current_boundary() -> JsResult<usize> {
    let marker = 0_u32;
    let current = std::ptr::addr_of!(marker) as usize;
    let (mut low, mut high) = (0, 0);
    // SAFETY: valid writable usize outputs, called on the isolate owner.
    // https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getcurrentthreadstacklimits
    unsafe {
        windows_sys::Win32::System::Threading::GetCurrentThreadStackLimits(&mut low, &mut high);
    }
    policy::boundary(low, high, current).ok_or_else(|| JsError {
        kind: JsErrorKind::Range,
        message: "JavaScript owner thread has insufficient or unrecognized stack space".into(),
    })
}

#[cfg(all(test, windows))]
mod tests;
