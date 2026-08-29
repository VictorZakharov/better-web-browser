use crate::limits::{MEDIA_COMMAND_TIMEOUT, MEDIA_SHUTDOWN_TIMEOUT, MEDIA_STARTUP_TIMEOUT};
use crate::media_protocol::{MediaSessionId, Nonce};
use crate::renderer_process::launcher::contained_environment;
use crate::renderer_process::windows::{
    AppContainerSid, InheritedInputPipe, LaunchAttributes, PipeSet, create_media_job, last_error,
    random_nonce, raw,
};
use std::fs::{self, File};
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::OwnedHandle;
use std::path::{Path, PathBuf};
use std::ptr::null;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, EXTENDED_STARTUPINFO_PRESENT,
    PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

static NEXT_MEDIA_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaStartupFault {
    Silent,
    WrongNonce,
    MalformedFrame,
    OversizedFrame,
    IncompatibleVersion,
}

#[derive(Clone, Debug)]
pub struct MediaLaunchOptions {
    pub executable: PathBuf,
    pub startup_timeout: Duration,
    pub command_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub test_mode: bool,
    pub startup_fault: Option<MediaStartupFault>,
}

impl MediaLaunchOptions {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            startup_timeout: MEDIA_STARTUP_TIMEOUT,
            command_timeout: MEDIA_COMMAND_TIMEOUT,
            shutdown_timeout: MEDIA_SHUTDOWN_TIMEOUT,
            test_mode: false,
            startup_fault: None,
        }
    }

    pub fn current_executable() -> Result<Self, String> {
        std::env::current_exe()
            .map(Self::new)
            .map_err(|error| format!("locate media executable: {error}"))
    }
}

pub(super) struct LaunchedMediaWorker {
    pub(super) process: OwnedHandle,
    pub(super) job: OwnedHandle,
    pub(super) browser_input: File,
    pub(super) browser_output: File,
    pub(super) browser_data_output: File,
    pub(super) process_id: u32,
    pub(super) session: MediaSessionId,
    pub(super) nonce: Nonce,
}

pub(super) fn launch(options: &MediaLaunchOptions) -> Result<LaunchedMediaWorker, String> {
    validate_executable(&options.executable)?;
    let nonce = random_nonce()?;
    let session_value = NEXT_MEDIA_SESSION.fetch_add(1, Ordering::Relaxed);
    let session = MediaSessionId::new(session_value)
        .map_err(|error| format!("allocate media session: {error}"))?;
    let pipes = PipeSet::create()?;
    let data_pipe = InheritedInputPipe::create("browser-to-media data")?;
    let job = create_media_job()?;
    let sid = AppContainerSid::create_media()?;
    let attributes = LaunchAttributes::with_inherited(
        &pipes.child_input,
        &pipes.child_output,
        &[&data_pipe.child_input],
        &job,
        &sid,
    )?;

    let application = wide_path(&options.executable);
    let mut command_line = wide(&command_line(
        options,
        nonce,
        session,
        raw(&data_pipe.child_input) as usize,
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
    let flags = CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT;
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
        return Err(last_error("launch contained media worker"));
    }
    unsafe { CloseHandle(process.hThread) };
    let process_handle = unsafe {
        use std::os::windows::io::{FromRawHandle, RawHandle};
        OwnedHandle::from_raw_handle(process.hProcess as RawHandle)
    };
    drop(pipes.child_input);
    drop(pipes.child_output);
    drop(data_pipe.child_input);
    Ok(LaunchedMediaWorker {
        process: process_handle,
        job,
        browser_input: File::from(pipes.browser_input),
        browser_output: File::from(pipes.browser_output),
        browser_data_output: File::from(data_pipe.browser_output),
        process_id: process.dwProcessId,
        session,
        nonce,
    })
}

fn validate_executable(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("media executable path must be absolute".into());
    }
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!(
                "media executable does not exist: {}: {error}",
                path.display()
            )
        } else {
            format!("inspect media executable {}: {error}", path.display())
        }
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "media executable is not a file: {}",
            path.display()
        ));
    }
    Ok(())
}

fn command_line(
    options: &MediaLaunchOptions,
    nonce: Nonce,
    session: MediaSessionId,
    data_handle: usize,
) -> String {
    let mut command = format!(
        "\"{}\" --media-process --media-nonce {} --media-session {} --media-data-handle {}",
        options.executable.display(),
        nonce.to_hex(),
        session.get(),
        data_handle,
    );
    if options.test_mode {
        command.push_str(" --media-test-mode");
    }
    if let Some(fault) = options.startup_fault {
        command.push_str(" --media-startup-fault ");
        command.push_str(match fault {
            MediaStartupFault::Silent => "silent",
            MediaStartupFault::WrongNonce => "wrong-nonce",
            MediaStartupFault::MalformedFrame => "malformed",
            MediaStartupFault::OversizedFrame => "oversized",
            MediaStartupFault::IncompatibleVersion => "incompatible",
        });
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
    fn command_line_is_hidden_role_bound_and_nonce_bound() {
        let options = MediaLaunchOptions::new(PathBuf::from("C:\\Breeze\\browser.exe"));
        let nonce = Nonce::new([3; 32]);
        let line = command_line(&options, nonce, MediaSessionId::new(7).unwrap(), 42);
        assert!(line.contains("--media-process"));
        assert!(line.contains("--media-nonce"));
        assert!(line.contains("--media-session 7"));
        assert!(line.contains("--media-data-handle 42"));
        assert!(!line.contains("--benchmark"));
    }
}
