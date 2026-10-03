use std::fs::File;
use std::os::windows::io::{BorrowedHandle, RawHandle};
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
};

unsafe extern "C" {
    fn _dup2(source: i32, destination: i32) -> i32;
    fn _write(descriptor: i32, buffer: *const std::ffi::c_void, count: u32) -> i32;
}

pub(super) fn isolate_protocol_output(output: HANDLE) -> Result<File, String> {
    let diagnostics = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    if diagnostics.is_null() || diagnostics == INVALID_HANDLE_VALUE || diagnostics == output {
        return Err("renderer diagnostics handle is invalid or aliases IPC".into());
    }
    // The CRT owns fd 1 and may close its HANDLE when _dup2 replaces it. Keep a distinct
    // owned duplicate exclusively for FrameWriter before redirecting native stdout.
    let protocol = unsafe { BorrowedHandle::borrow_raw(output as RawHandle) }
        .try_clone_to_owned()
        .map_err(|error| format!("duplicate private renderer IPC output: {error}"))?;
    // MSVC _dup2 changes cached FILE/stdout descriptors; SetStdHandle alone only changes
    // future GetStdHandle lookups. Cover both native-library and Rust diagnostic writes.
    // https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/dup-dup2
    if unsafe { _dup2(2, 1) } != 0 {
        return Err(format!(
            "redirect renderer native stdout: {}",
            std::io::Error::last_os_error()
        ));
    }
    if unsafe { SetStdHandle(STD_OUTPUT_HANDLE, diagnostics) } == 0 {
        return Err(format!(
            "redirect renderer standard output: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(File::from(protocol))
}

// This path is reachable only through the broker's explicit renderer-test-mode
// gate. Exercise cached CRT descriptors, rather than Rust's GetStdHandle path.
pub(super) fn test_native_diagnostics() -> Result<(), String> {
    let diagnostic = b"contained native diagnostic\n";
    for descriptor in [1, 2] {
        let count = unsafe {
            _write(
                descriptor,
                diagnostic.as_ptr().cast(),
                diagnostic.len() as u32,
            )
        };
        if count != diagnostic.len() as i32 {
            return Err(format!(
                "native diagnostic descriptor {descriptor} write failed"
            ));
        }
    }
    Ok(())
}
