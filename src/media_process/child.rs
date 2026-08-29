use super::backend;
use super::launcher::MediaStartupFault;
use crate::media_protocol::{
    BrowserMediaMessage, ContainmentReport, MEDIA_HEADER_LENGTH, MEDIA_MAGIC, MEDIA_PROTOCOL_MAJOR,
    MEDIA_PROTOCOL_MINOR, MediaFrameReader, MediaFrameWriter, MediaLimits, MediaRestrictionReport,
    MediaSessionId, MediaTestCommand, Nonce, WorkerMediaMessage,
};
use std::fs::File;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::null_mut;
use std::time::Duration;
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TokenIsAppContainer};
use windows_sys::Win32::System::Console::{
    GetConsoleWindow, GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, GetCurrentProcess, OpenProcessToken,
};

pub(super) fn run(arguments: &[String]) -> Result<(), String> {
    let options = ChildOptions::parse(arguments)?;
    let input_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let output_handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if !valid_handle(input_handle) || !valid_handle(output_handle) {
        return Err("media IPC standard handles are invalid".into());
    }
    let input = unsafe { File::from_raw_handle(input_handle as RawHandle) };
    let output = unsafe { File::from_raw_handle(output_handle as RawHandle) };
    run_protocol(input, output, options)
}

fn run_protocol(input: File, output: File, options: ChildOptions) -> Result<(), String> {
    if options.fault == Some(MediaStartupFault::Silent) {
        std::thread::sleep(Duration::from_secs(60));
        return Ok(());
    }
    let mut reader = MediaFrameReader::new(input, options.session);
    let BrowserMediaMessage::Hello { nonce, limits } = reader
        .read_browser()
        .map_err(|error| format!("read media hello: {error}"))?
    else {
        return Err("media worker expected Hello as its first message".into());
    };
    if nonce != options.nonce {
        return Err("media bootstrap nonce mismatch".into());
    }
    limits.validate().map_err(|error| error.to_string())?;

    let mut writer = MediaFrameWriter::new(output, options.session);
    match options.fault {
        Some(MediaStartupFault::WrongNonce) => {
            writer
                .send_worker(&WorkerMediaMessage::Ready {
                    nonce: Nonce::new([0; 32]),
                    containment: containment_report()?,
                })
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        Some(MediaStartupFault::MalformedFrame) => {
            writer
                .into_inner()
                .write_all(&[0; MEDIA_HEADER_LENGTH])
                .map_err(|error| format!("write malformed media test frame: {error}"))?;
            return Ok(());
        }
        Some(MediaStartupFault::OversizedFrame) => {
            write_raw_header(
                writer.into_inner(),
                options.session,
                MEDIA_PROTOCOL_MAJOR,
                (crate::limits::MAX_MEDIA_CONTROL_PAYLOAD + 1) as u32,
            )?;
            return Ok(());
        }
        Some(MediaStartupFault::IncompatibleVersion) => {
            write_raw_header(
                writer.into_inner(),
                options.session,
                MEDIA_PROTOCOL_MAJOR + 1,
                0,
            )?;
            return Ok(());
        }
        Some(MediaStartupFault::Silent) => unreachable!(),
        None => {}
    }
    writer
        .send_worker(&WorkerMediaMessage::Ready {
            nonce,
            containment: containment_report()?,
        })
        .map_err(|error| error.to_string())?;
    command_loop(&mut reader, &mut writer, limits, options.test_mode)
}

fn command_loop(
    reader: &mut MediaFrameReader<File>,
    writer: &mut MediaFrameWriter<File>,
    limits: MediaLimits,
    test_mode: bool,
) -> Result<(), String> {
    loop {
        match reader
            .read_browser()
            .map_err(|error| format!("read media command: {error}"))?
        {
            BrowserMediaMessage::Ping(token) => writer
                .send_worker(&WorkerMediaMessage::Pong(token))
                .map_err(|error| error.to_string())?,
            BrowserMediaMessage::Shutdown => {
                writer
                    .send_worker(&WorkerMediaMessage::ShutdownComplete)
                    .map_err(|error| error.to_string())?;
                return Ok(());
            }
            BrowserMediaMessage::Probe { request_id } => {
                let report = backend::probe(limits);
                writer
                    .send_worker(&WorkerMediaMessage::Capability { request_id, report })
                    .map_err(|error| error.to_string())?;
            }
            BrowserMediaMessage::Test(command) if test_mode => {
                handle_test(command, writer)?;
            }
            BrowserMediaMessage::Test(_) => return Err("media test command denied".into()),
            BrowserMediaMessage::Hello { .. } => {
                return Err("media worker received a duplicate Hello".into());
            }
        }
    }
}

fn handle_test(
    command: MediaTestCommand,
    writer: &mut MediaFrameWriter<File>,
) -> Result<(), String> {
    match command {
        MediaTestCommand::Crash => std::process::abort(),
        MediaTestCommand::Hang => loop {
            std::thread::sleep(Duration::from_secs(60));
        },
        MediaTestCommand::DelayResponse { millis } => {
            std::thread::sleep(Duration::from_millis(u64::from(millis)));
            writer
                .send_worker(&WorkerMediaMessage::Pong(0))
                .map_err(|error| error.to_string())
        }
        MediaTestCommand::WriteMalformedFrame => {
            writer
                .inner_mut()
                .write_all(&[0; MEDIA_HEADER_LENGTH])
                .map_err(|error| format!("write malformed media frame: {error}"))?;
            Err("injected malformed media frame".into())
        }
        MediaTestCommand::ProbeRestrictions { loopback_port } => writer
            .send_worker(&WorkerMediaMessage::Restrictions(probe_restrictions(
                loopback_port,
            )))
            .map_err(|error| error.to_string()),
    }
}

fn containment_report() -> Result<ContainmentReport, String> {
    Ok(ContainmentReport {
        app_container: is_app_container()?,
        no_console_window: unsafe { GetConsoleWindow() }.is_null(),
        minimal_environment: has_minimal_environment(),
    })
}

fn has_minimal_environment() -> bool {
    let mut system_root = false;
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.eq_ignore_ascii_case("SystemRoot") {
            system_root = true;
        }
        if !crate::renderer_process::renderer_environment_name_allowed(&name) {
            return false;
        }
    }
    system_root
}

fn is_app_container() -> Result<bool, String> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err("open media process token".into());
    }
    let mut app_container = 0_u32;
    let mut returned = 0_u32;
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenIsAppContainer,
            (&mut app_container as *mut u32).cast(),
            size_of::<u32>() as u32,
            &mut returned,
        )
    };
    unsafe { windows_sys::Win32::Foundation::CloseHandle(token) };
    (ok != 0 && returned as usize == size_of::<u32>())
        .then_some(app_container != 0)
        .ok_or_else(|| "query media AppContainer token".into())
}

fn probe_restrictions(loopback_port: u16) -> MediaRestrictionReport {
    let child_error = probe_child_launch();
    let loopback_error = probe_network(SocketAddr::from(([127, 0, 0, 1], loopback_port)));
    let internet_error = probe_network(SocketAddr::from(([1, 1, 1, 1], 443)));
    MediaRestrictionReport {
        child_launch_denied: is_access_denied(child_error),
        loopback_denied: loopback_error != 0,
        internet_denied: is_access_denied(internet_error),
        child_error,
        loopback_error,
        internet_error,
    }
}

fn probe_child_launch() -> i32 {
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => return error.raw_os_error().unwrap_or(-1),
    };
    match Command::new(executable)
        .arg("--media-child-probe")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(mut child) => {
            let _ = child.kill();
            let _ = child.wait();
            0
        }
        Err(error) => error.raw_os_error().unwrap_or(-1),
    }
}

fn probe_network(address: SocketAddr) -> i32 {
    // The diagnostic must remain comfortably inside the broker's bounded command deadline even
    // when Windows reports an isolated route by timing out rather than returning access denied.
    match TcpStream::connect_timeout(&address, Duration::from_millis(200)) {
        Ok(_) => 0,
        Err(error) => error.raw_os_error().unwrap_or_else(|| match error.kind() {
            std::io::ErrorKind::PermissionDenied => -2,
            std::io::ErrorKind::TimedOut => -3,
            _ => -1,
        }),
    }
}

fn is_access_denied(error: i32) -> bool {
    matches!(error, 5 | 10_013)
}

fn write_raw_header(
    mut output: File,
    session: MediaSessionId,
    major: u16,
    payload_length: u32,
) -> Result<(), String> {
    let mut header = [0_u8; MEDIA_HEADER_LENGTH];
    header[..4].copy_from_slice(&MEDIA_MAGIC);
    header[4..6].copy_from_slice(&major.to_le_bytes());
    header[6..8].copy_from_slice(&MEDIA_PROTOCOL_MINOR.to_le_bytes());
    header[8..10].copy_from_slice(&2_u16.to_le_bytes());
    header[12..16].copy_from_slice(&payload_length.to_le_bytes());
    header[16..24].copy_from_slice(&session.get().to_le_bytes());
    header[24..32].copy_from_slice(&1_u64.to_le_bytes());
    output
        .write_all(&header)
        .map_err(|error| format!("write raw media test frame: {error}"))
}

fn valid_handle(handle: HANDLE) -> bool {
    !handle.is_null() && handle != INVALID_HANDLE_VALUE
}

use std::mem::size_of;

struct ChildOptions {
    nonce: Nonce,
    session: MediaSessionId,
    test_mode: bool,
    fault: Option<MediaStartupFault>,
}

impl ChildOptions {
    fn parse(arguments: &[String]) -> Result<Self, String> {
        let value = |name: &str| {
            arguments
                .iter()
                .position(|argument| argument == name)
                .and_then(|index| arguments.get(index + 1))
                .ok_or_else(|| format!("{name} requires a value"))
        };
        let nonce = Nonce::from_hex(value("--media-nonce")?)
            .map_err(|error| format!("parse media nonce: {error}"))?;
        let session = value("--media-session")?
            .parse::<u64>()
            .map_err(|_| "--media-session requires an integer".to_string())
            .and_then(|value| MediaSessionId::new(value).map_err(|error| error.to_string()))?;
        let fault = arguments
            .iter()
            .any(|argument| argument == "--media-startup-fault")
            .then(|| value("--media-startup-fault"))
            .transpose()?
            .map(|fault| match fault.as_str() {
                "silent" => Ok(MediaStartupFault::Silent),
                "wrong-nonce" => Ok(MediaStartupFault::WrongNonce),
                "malformed" => Ok(MediaStartupFault::MalformedFrame),
                "oversized" => Ok(MediaStartupFault::OversizedFrame),
                "incompatible" => Ok(MediaStartupFault::IncompatibleVersion),
                _ => Err(format!("unknown media startup fault: {fault}")),
            })
            .transpose()?;
        Ok(Self {
            nonce,
            session,
            test_mode: arguments
                .iter()
                .any(|argument| argument == "--media-test-mode"),
            fault,
        })
    }
}
