//! Pointer-free metadata for a bounded, producer-owned DXGI presentation surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuFrame {
    pub surface: u64,
    pub handle: u64,
    pub key: u64,
    pub width: u32,
    pub height: u32,
    pub adapter_low: u32,
    pub adapter_high: i32,
}

impl GpuFrame {
    pub fn validate(self) -> bool {
        self.surface != 0
            && self.handle != 0
            && self.key != 0
            && self.width != 0
            && self.height != 0
            && self.width <= 4096
            && self.height <= 4096
            && u64::from(self.width) * u64::from(self.height) <= 4 * 1024 * 1024
            && self.handle <= usize::MAX as u64
    }
}

#[cfg(windows)]
pub(crate) fn try_acquire(
    mutex: &windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex,
    key: u64,
) -> Result<bool, String> {
    use windows::core::Interface;
    // WAIT_TIMEOUT and WAIT_ABANDONED are positive HRESULTs. Result::is_ok() is
    // insufficient; only S_OK owns the resource. Never wait on the UI thread.
    // https://learn.microsoft.com/windows/win32/api/dxgi/nf-dxgi-idxgikeyedmutex-acquiresync
    let result = unsafe { (mutex.vtable().AcquireSync)(mutex.as_raw(), key, 0) }.0;
    acquire_result(result)
}

#[cfg(windows)]
fn acquire_result(result: i32) -> Result<bool, String> {
    match result {
        0 => Ok(true),
        258 => Ok(false),
        _ => Err(format!(
            "shared GPU surface acquisition failed: {result:#x}"
        )),
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn keyed_mutex_timeout_and_abandonment_do_not_grant_ownership() {
        assert_eq!(super::acquire_result(0), Ok(true));
        assert_eq!(super::acquire_result(258), Ok(false));
        assert!(super::acquire_result(128).is_err());
        assert!(super::acquire_result(1).is_err());
        assert!(super::acquire_result(-1).is_err());
    }
}
