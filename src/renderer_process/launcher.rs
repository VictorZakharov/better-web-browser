use super::windows::{
    AppContainerSid, LaunchAttributes, PipeSet, create_renderer_job, last_error, random_nonce, raw,
};
use crate::renderer_protocol::{Nonce, RendererSessionId};
use std::fs::File;
use std::mem::size_of;
use std::os::windows::io::OwnedHandle;
use std::path::{Path, PathBuf};
use std::ptr::null;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateProcessW, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupFault {
    Silent,
    WrongNonce,
    MalformedFrame,
    OversizedFrame,
    IncompatibleVersion,
}

#[derive(Clone, Debug)]
pub struct RendererLaunchOptions {
    pub executable: PathBuf,
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub unresponsive_timeout: Duration,
    pub test_mode: bool,
    pub startup_fault: Option<StartupFault>,
}

impl RendererLaunchOptions {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            startup_timeout: Duration::from_secs(5),
            shutdown_timeout: Duration::from_secs(2),
            heartbeat_interval: Duration::from_secs(1),
            unresponsive_timeout: Duration::from_secs(10),
            test_mode: false,
            startup_fault: None,
        }
    }

    pub fn current_executable() -> Result<Self, String> {
        std::env::current_exe()
            .map(Self::new)
            .map_err(|error| format!("locate renderer executable: {error}"))
    }
}

pub(super) struct LaunchedRenderer {
    pub(super) process: OwnedHandle,
    pub(super) job: OwnedHandle,
    pub(super) browser_input: File,
    pub(super) browser_output: File,
    pub(super) process_id: u32,
    pub(super) session: RendererSessionId,
    pub(super) nonce: Nonce,
}

pub(super) fn launch(options: &RendererLaunchOptions) -> Result<LaunchedRenderer, String> {
    validate_executable(&options.executable)?;
    let nonce = random_nonce()?;
    let session_value = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let session = RendererSessionId::new(session_value)
        .map_err(|error| format!("allocate renderer session: {error}"))?;
    let pipes = PipeSet::create()?;
    let job = create_renderer_job()?;
    let sid = AppContainerSid::create_or_open()?;
    let attributes = LaunchAttributes::new(&pipes.child_input, &pipes.child_output, &job, &sid)?;

    let application = wide_path(&options.executable);
    let mut command_line = wide(&command_line(options, nonce, session));
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = raw(&pipes.child_input);
    startup.StartupInfo.hStdOutput = raw(&pipes.child_output);
    // Reusing the protocol output keeps the allowlist at exactly two unique handles. Renderer mode
    // installs no stderr diagnostics or panic output, so only framed writes reach this pipe.
    startup.StartupInfo.hStdError = raw(&pipes.child_output);
    startup.lpAttributeList = attributes.as_ptr();
    let mut process = PROCESS_INFORMATION::default();
    let flags = CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT;
    // Stage 2 sends no remote document or resource bytes to this child. Before that changes, replace
    // this inherited environment with an audited minimal block so page code cannot observe browser
    // credentials or configuration through environment variables.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            flags,
            null(),
            null(),
            &startup.StartupInfo,
            &mut process,
        )
    };
    if created == 0 {
        return Err(last_error("launch contained renderer"));
    }
    unsafe {
        CloseHandle(process.hThread);
    }
    // SAFETY: CreateProcessW returned an owned process handle.
    let process_handle = unsafe {
        use std::os::windows::io::{FromRawHandle, RawHandle};
        OwnedHandle::from_raw_handle(process.hProcess as RawHandle)
    };
    drop(pipes.child_input);
    drop(pipes.child_output);
    Ok(LaunchedRenderer {
        process: process_handle,
        job,
        browser_input: File::from(pipes.browser_input),
        browser_output: File::from(pipes.browser_output),
        process_id: process.dwProcessId,
        session,
        nonce,
    })
}

fn validate_executable(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("renderer executable path must be absolute".into());
    }
    if !path.is_file() {
        return Err(format!(
            "renderer executable does not exist: {}",
            path.display()
        ));
    }
    Ok(())
}

fn command_line(
    options: &RendererLaunchOptions,
    nonce: Nonce,
    session: RendererSessionId,
) -> String {
    let mut command = format!(
        "\"{}\" --renderer-process --renderer-nonce {} --renderer-session {}",
        options.executable.display(),
        nonce.to_hex(),
        session.get()
    );
    if options.test_mode {
        command.push_str(" --renderer-test-mode");
    }
    if let Some(fault) = options.startup_fault {
        command.push_str(" --renderer-startup-fault ");
        command.push_str(match fault {
            StartupFault::Silent => "silent",
            StartupFault::WrongNonce => "wrong-nonce",
            StartupFault::MalformedFrame => "malformed",
            StartupFault::OversizedFrame => "oversized",
            StartupFault::IncompatibleVersion => "incompatible",
        });
    }
    command
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
