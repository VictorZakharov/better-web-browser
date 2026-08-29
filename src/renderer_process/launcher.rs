use super::windows::{
    AppContainerSid, LaunchAttributes, PipeSet, create_renderer_job, last_error, random_nonce, raw,
};
use crate::limits::{
    RENDERER_FIRST_PRESENTATION_TIMEOUT, RENDERER_HEARTBEAT_INTERVAL, RENDERER_SHUTDOWN_TIMEOUT,
    RENDERER_STARTUP_TIMEOUT, RENDERER_UNRESPONSIVE_KILL_TIMEOUT, RENDERER_UNRESPONSIVE_TIMEOUT,
};
use crate::renderer_protocol::{BrowsingContextId, Nonce, RendererSessionId};
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
    pub browsing_context: BrowsingContextId,
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub unresponsive_timeout: Duration,
    pub unresponsive_kill_timeout: Duration,
    pub first_presentation_timeout: Duration,
    pub test_mode: bool,
    pub startup_fault: Option<StartupFault>,
}

impl RendererLaunchOptions {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            browsing_context: BrowsingContextId::new(1).expect("default browsing context"),
            startup_timeout: RENDERER_STARTUP_TIMEOUT,
            shutdown_timeout: RENDERER_SHUTDOWN_TIMEOUT,
            heartbeat_interval: RENDERER_HEARTBEAT_INTERVAL,
            unresponsive_timeout: RENDERER_UNRESPONSIVE_TIMEOUT,
            unresponsive_kill_timeout: RENDERER_UNRESPONSIVE_KILL_TIMEOUT,
            first_presentation_timeout: RENDERER_FIRST_PRESENTATION_TIMEOUT,
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
    let sid = AppContainerSid::create_renderer()?;
    let attributes = LaunchAttributes::new(&pipes.child_input, &pipes.child_output, &job, &sid)?;

    let application = wide_path(&options.executable);
    let mut command_line = wide(&command_line(options, nonce, session));
    let environment = contained_environment(&options.executable)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = raw(&pipes.child_input);
    startup.StartupInfo.hStdOutput = raw(&pipes.child_output);
    // Reusing the protocol output keeps the allowlist at exactly two unique handles. Renderer mode
    // suppresses panic output; fatal fault-injection paths terminate without writing diagnostics.
    startup.StartupInfo.hStdError = raw(&pipes.child_output);
    startup.lpAttributeList = attributes.as_ptr();
    let mut process = PROCESS_INFORMATION::default();
    let flags = CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT;
    // CreateProcessW requires an explicit double-NUL-terminated UTF-16 environment block when
    // CREATE_UNICODE_ENVIRONMENT is set. Only Windows bootstrap variables cross this boundary;
    // browser credentials, proxy settings, developer tokens, and user configuration do not.
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
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!(
                "renderer executable does not exist: {}: {error}",
                path.display()
            )
        } else {
            format!("inspect renderer executable {}: {error}", path.display())
        }
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "renderer executable is not a file: {}",
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
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub(crate) fn contained_environment(executable: &Path) -> Result<Vec<u16>, String> {
    let mut entries = super::RENDERER_ENVIRONMENT_ALLOWLIST
        .iter()
        .filter_map(|name| std::env::var_os(name).map(|value| ((*name).to_string(), value)))
        .collect::<Vec<_>>();
    for directory in [std::env::current_dir().ok().as_deref(), executable.parent()]
        .into_iter()
        .flatten()
    {
        if let Some(drive) = windows_drive(directory) {
            entries.push((format!("={drive}:"), directory.as_os_str().to_owned()));
        }
    }
    entries.sort_by_key(|(name, _)| name.to_ascii_uppercase());
    entries.dedup_by(|left, right| left.0.eq_ignore_ascii_case(&right.0));
    if !entries
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("SystemRoot"))
    {
        return Err("renderer launch requires SystemRoot".into());
    }

    let mut block = Vec::new();
    for (name, value) in entries {
        if !super::renderer_environment_name_allowed(&name) {
            return Err(format!(
                "renderer environment variable {name} is not allowlisted"
            ));
        }
        let prefix = std::ffi::OsString::from(format!("{name}="));
        for unit in prefix.encode_wide().chain(value.encode_wide()) {
            if unit == 0 {
                return Err(format!("renderer environment variable {name} contains NUL"));
            }
            block.push(unit);
        }
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn windows_drive(path: &Path) -> Option<char> {
    let text = path.as_os_str().to_string_lossy();
    let mut characters = text.chars();
    let drive = characters.next()?;
    (drive.is_ascii_alphabetic() && characters.next() == Some(':'))
        .then(|| drive.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_environment_is_allowlisted_and_double_terminated() {
        let executable = std::env::current_exe().unwrap();
        let block = contained_environment(&executable).unwrap();
        assert!(block.ends_with(&[0, 0]));
        for entry in block[..block.len() - 1]
            .split(|unit| *unit == 0)
            .filter(|entry| !entry.is_empty())
        {
            let entry = String::from_utf16(entry).unwrap();
            let split = entry[1..].find('=').map(|index| index + 1).unwrap();
            assert!(super::super::renderer_environment_name_allowed(
                &entry[..split]
            ));
        }
    }

    #[test]
    fn executable_validation_distinguishes_missing_and_non_file_paths() {
        let executable = std::env::current_exe().unwrap();
        validate_executable(&executable).unwrap();

        let missing = executable.with_file_name("missing-renderer-executable.exe");
        assert!(
            validate_executable(&missing)
                .unwrap_err()
                .starts_with("renderer executable does not exist:")
        );
        assert!(
            validate_executable(executable.parent().unwrap())
                .unwrap_err()
                .starts_with("renderer executable is not a file:")
        );
    }
}
