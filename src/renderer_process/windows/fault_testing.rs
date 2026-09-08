use windows_sys::Win32::System::Diagnostics::Debug::{
    GetErrorMode, SEM_NOGPFAULTERRORBOX, SetErrorMode,
};

/// Call only inside a worker immediately before an explicitly requested test crash.
pub(crate) fn suppress_injected_fault_reporting() {
    // WER/debugger processing can retain a deliberately crashed process and its pipes.
    // Disable it locally for fault injection, not for real failures or the user's system.
    // https://devblogs.microsoft.com/oldnewthing/20230227-00/?p=107875
    // SAFETY: changes only this doomed test child's error mode and preserves other flags.
    unsafe { SetErrorMode(GetErrorMode() | SEM_NOGPFAULTERRORBOX) };
}
