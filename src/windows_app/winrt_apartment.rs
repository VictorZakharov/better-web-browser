//! Scoped Windows Runtime apartment initialization for native browser services.

use windows::Win32::System::WinRT::{
    RO_INIT_MULTITHREADED, RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
};

pub(super) struct WinRtApartment;

impl WinRtApartment {
    pub(super) fn initialize_ui() -> Result<Self, String> {
        // The browser's HWND message loop is an STA. Keep the guard alive until
        // after its windows and WinRT services have been destroyed.
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }
            .map_err(|error| format!("initialize browser Windows Runtime apartment: {error}"))?;
        Ok(Self)
    }

    pub(super) fn initialize_worker() -> windows::core::Result<Self> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
        Ok(Self)
    }

    pub(super) fn initialize_callback() -> windows::core::Result<Option<Self>> {
        match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            Ok(()) => Ok(Some(Self)),
            // WinRT may invoke a completed handler synchronously on the
            // calling STA; it already has an apartment to use.
            Err(error) if error.code().0 as u32 == 0x8001_0106 => Ok(None),
            Err(error) => Err(error),
        }
    }
}

impl Drop for WinRtApartment {
    fn drop(&mut self) {
        // Every successful RoInitialize, including S_FALSE, needs one balance.
        unsafe { RoUninitialize() };
    }
}
