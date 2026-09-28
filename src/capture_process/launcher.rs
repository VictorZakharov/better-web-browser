use crate::capture_protocol::{CaptureDevices, CaptureSessionId};
use crate::renderer_process::launcher::contained_environment;
use crate::renderer_process::windows::{
    AppContainerSid, InheritedOutputPipe, LaunchAttributes, PipeSet, create_capture_job,
    last_error, random_nonce, raw,
};
use crate::renderer_protocol::Nonce;
use std::fs::{self, File};
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::OwnedHandle;
use std::path::{Path, PathBuf};
use std::ptr::null;
use std::sync::atomic::{AtomicU64, Ordering};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, EXTENDED_STARTUPINFO_PRESENT,
    PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

static NEXT_CAPTURE_SESSION: AtomicU64 = AtomicU64::new(1);

fn creation_flags() -> u32 {
    CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT
}

pub(crate) struct CaptureLaunchOptions {
    pub(crate) executable: PathBuf,
    pub(crate) devices: CaptureDevices,
    /// Explicit fixture mode for the hidden AppContainer process test. Never set by web content.
    pub(crate) test_mode: bool,
}

impl CaptureLaunchOptions {
    pub(crate) fn new(executable: impl Into<PathBuf>, devices: CaptureDevices) -> Self {
        Self {
            executable: executable.into(),
            devices,
            test_mode: false,
        }
    }
}

pub(crate) struct LaunchedCapture {
    pub(crate) process: OwnedHandle,
    pub(crate) job: OwnedHandle,
    pub(crate) browser_input: File,
    pub(crate) browser_output: File,
    pub(crate) sample_input: File,
    pub(crate) session: CaptureSessionId,
    pub(crate) nonce: Nonce,
    pub(crate) devices: CaptureDevices,
}

pub(crate) fn launch(options: &CaptureLaunchOptions) -> Result<LaunchedCapture, String> {
    options
        .devices
        .validate()
        .map_err(|error| error.to_string())?;
    validate_executable(&options.executable)?;
    let nonce = random_nonce()?;
    let session = CaptureSessionId::new(NEXT_CAPTURE_SESSION.fetch_add(1, Ordering::Relaxed))
        .map_err(|error| error.to_string())?;
    let pipes = PipeSet::create()?;
    let sample_pipe = InheritedOutputPipe::create("capture sample")?;
    let job = create_capture_job()?;
    let sid = AppContainerSid::create_capture()?;
    let attributes = LaunchAttributes::with_capture_capabilities(
        &pipes.child_input,
        &pipes.child_output,
        &[raw(&sample_pipe.child_output)],
        &job,
        &sid,
        options.devices.camera,
        options.devices.microphone,
    )?;
    let application = wide_path(&options.executable);
    let mut command_line = wide(&command_line(
        options,
        nonce,
        session,
        raw(&sample_pipe.child_output) as usize,
    ));
    let environment = contained_environment(&options.executable)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = raw(&pipes.child_input);
    startup.StartupInfo.hStdOutput = raw(&pipes.child_output);
    startup.StartupInfo.hStdError = raw(&pipes.child_output);
    startup.lpAttributeList = attributes.as_ptr();
    let mut process = PROCESS_INFORMATION::default();
    let flags = creation_flags();
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            flags,
            environment.as_ptr().cast(),
            null(),
            &startup.StartupInfo,
            &mut process,
        )
    };
    if created == 0 {
        return Err(last_error("launch contained capture worker"));
    }
    unsafe { CloseHandle(process.hThread) };
    let process_handle = unsafe {
        use std::os::windows::io::{FromRawHandle, RawHandle};
        OwnedHandle::from_raw_handle(process.hProcess as RawHandle)
    };
    drop(pipes.child_input);
    drop(pipes.child_output);
    drop(sample_pipe.child_output);
    Ok(LaunchedCapture {
        process: process_handle,
        job,
        browser_input: File::from(pipes.browser_input),
        browser_output: File::from(pipes.browser_output),
        sample_input: File::from(sample_pipe.browser_input),
        session,
        nonce,
        devices: options.devices,
    })
}

fn validate_executable(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("capture executable path must be absolute".into());
    }
    let metadata = fs::metadata(path)
        .map_err(|error| format!("inspect capture executable {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "capture executable is not a file: {}",
            path.display()
        ));
    }
    Ok(())
}

fn command_line(
    options: &CaptureLaunchOptions,
    nonce: Nonce,
    session: CaptureSessionId,
    sample_handle: usize,
) -> String {
    let mut command = format!(
        "\"{}\" --capture-process --capture-nonce {} --capture-session {} --capture-sample-handle {} --capture-devices {}",
        options.executable.display(),
        nonce.to_hex(),
        session.get(),
        sample_handle,
        options.devices.bits(),
    );
    if options.test_mode {
        command.push_str(" --capture-test-mode");
    }
    command
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_is_role_and_grant_bound() {
        let options = CaptureLaunchOptions::new(
            PathBuf::from("C:\\Breeze\\browser.exe"),
            CaptureDevices {
                camera: true,
                microphone: false,
            },
        );
        let line = command_line(
            &options,
            Nonce::new([2; 32]),
            CaptureSessionId::new(7).unwrap(),
            42,
        );
        assert!(line.contains("--capture-process"));
        assert!(line.contains("--capture-session 7"));
        assert!(line.contains("--capture-sample-handle 42"));
        assert!(line.contains("--capture-devices 1"));
        assert!(!line.contains("--capture-test-mode"));
    }

    #[test]
    fn capture_child_launch_is_hidden_and_has_explicit_handles() {
        assert_ne!(creation_flags() & CREATE_NO_WINDOW, 0);
        assert_ne!(creation_flags() & EXTENDED_STARTUPINFO_PRESENT, 0);
        let mut options = CaptureLaunchOptions::new(
            PathBuf::from("C:\\Breeze\\browser.exe"),
            CaptureDevices {
                camera: true,
                microphone: true,
            },
        );
        options.test_mode = true;
        let line = command_line(
            &options,
            Nonce::new([9; 32]),
            CaptureSessionId::new(9).unwrap(),
            41,
        );
        assert!(line.contains("--capture-process"));
        assert!(line.contains("--capture-test-mode"));
        assert!(line.contains("--capture-devices 3"));
        assert!(line.contains("--capture-sample-handle 41"));
    }
}
