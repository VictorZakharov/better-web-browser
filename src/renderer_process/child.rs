use crate::renderer_protocol::{
    BrowserMessage, ContainmentReport, FrameReader, FrameWriter, HEADER_LENGTH, MAGIC,
    MAX_CONTROL_PAYLOAD, Nonce, PROTOCOL_MAJOR, PROTOCOL_MINOR, RendererMessage, RendererSessionId,
    RestrictionReport, TestCommand,
};
use std::fs::File;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::{NonNull, null_mut};
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TokenIsAppContainer};
use windows_sys::Win32::System::Console::{
    GetConsoleWindow, GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, GetCurrentProcess, OpenProcessToken,
};

pub(super) const CHILD_EXIT_PROTOCOL_ERROR: i32 = 70;

pub(super) fn run(arguments: &[String]) -> Result<(), String> {
    std::panic::set_hook(Box::new(|_| {}));
    let options = ChildOptions::parse(arguments)?;
    let input_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let output_handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if !valid_handle(input_handle) || !valid_handle(output_handle) {
        return Err("renderer IPC standard handles are invalid".into());
    }
    // SAFETY: renderer mode owns the two allowlisted inherited standard handles until exit.
    let input = unsafe { File::from_raw_handle(input_handle as RawHandle) };
    let output = unsafe { File::from_raw_handle(output_handle as RawHandle) };
    run_protocol(input, output, options)
}

fn run_protocol(input: File, output: File, options: ChildOptions) -> Result<(), String> {
    if options.fault == Some(StartupFault::Silent) {
        std::thread::sleep(Duration::from_secs(60));
        return Ok(());
    }
    let mut reader = FrameReader::new(input, options.session);
    let hello = reader
        .read_browser()
        .map_err(|error| format!("read renderer hello: {error}"))?;
    let BrowserMessage::Hello { nonce, limits } = hello else {
        return Err("renderer expected Hello as its first message".into());
    };
    if nonce != options.nonce {
        return Err("renderer bootstrap nonce mismatch".into());
    }
    if limits.max_control_payload as usize > MAX_CONTROL_PAYLOAD {
        return Err("browser supplied an invalid control-message limit".into());
    }

    let mut writer = FrameWriter::new(output, options.session);
    match options.fault {
        Some(StartupFault::WrongNonce) => {
            writer
                .send_renderer(&RendererMessage::Ready {
                    nonce: Nonce::new([0; 32]),
                    containment: containment_report()?,
                })
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        Some(StartupFault::MalformedFrame) => {
            writer
                .into_inner()
                .write_all(&[0_u8; HEADER_LENGTH])
                .map_err(|error| format!("write malformed test frame: {error}"))?;
            return Ok(());
        }
        Some(StartupFault::OversizedFrame) => {
            write_raw_header(
                writer.into_inner(),
                options.session,
                PROTOCOL_MAJOR,
                (MAX_CONTROL_PAYLOAD + 1) as u32,
            )?;
            return Ok(());
        }
        Some(StartupFault::IncompatibleVersion) => {
            write_raw_header(writer.into_inner(), options.session, PROTOCOL_MAJOR + 1, 0)?;
            return Ok(());
        }
        Some(StartupFault::Silent) => unreachable!(),
        None => {}
    }
    writer
        .send_renderer(&RendererMessage::Ready {
            nonce,
            containment: containment_report()?,
        })
        .map_err(|error| error.to_string())?;
    connection::ChildConnection::new(reader, writer, options.test_mode).run()
}

pub(super) fn handle_test(
    command: TestCommand,
    writer: &mut FrameWriter<File>,
) -> Result<(), String> {
    match command {
        TestCommand::Crash => std::process::abort(),
        TestCommand::AccessViolation => raise_access_violation(),
        TestCommand::OutOfMemory => terminate_for_out_of_memory(),
        TestCommand::StackOverflow => {
            overflow_stack(0);
            unreachable!("stack-overflow injection returned")
        }
        TestCommand::Hang => loop {
            std::thread::sleep(Duration::from_secs(60));
        },
        TestCommand::WriteMalformedFrame => {
            writer
                .inner_mut()
                .write_all(&[0_u8; HEADER_LENGTH])
                .map_err(|error| format!("write malformed runtime frame: {error}"))?;
            Ok(())
        }
        TestCommand::ProbeRestrictions { loopback_port } => {
            let report = probe_restrictions(loopback_port);
            writer
                .send_renderer(&RendererMessage::Restrictions(report))
                .map_err(|error| error.to_string())
        }
    }
}

fn probe_restrictions(loopback_port: u16) -> RestrictionReport {
    let child_error = probe_child_launch();
    let loopback_error = probe_network(SocketAddr::from(([127, 0, 0, 1], loopback_port)));
    let internet_error = probe_network(SocketAddr::from(([1, 1, 1, 1], 443)));
    RestrictionReport {
        child_launch_denied: is_access_denied(child_error),
        // The browser owns a listening socket for this exact endpoint, so any failed connection
        // proves that the renderer could not reach loopback. Some Windows builds report this as a
        // policy timeout without preserving WSAEACCES in std::io::Error::raw_os_error.
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
        .arg("--renderer-child-probe")
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
    match TcpStream::connect_timeout(&address, Duration::from_millis(750)) {
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
        if !super::renderer_environment_name_allowed(&name) {
            return false;
        }
    }
    system_root
}

fn raise_access_violation() -> ! {
    const EXCEPTION_ACCESS_VIOLATION: u32 = 0xC000_0005;
    const EXCEPTION_NONCONTINUABLE: u32 = 1;
    unsafe extern "system" {
        fn RaiseException(code: u32, flags: u32, count: u32, arguments: *const usize);
    }
    unsafe {
        RaiseException(
            EXCEPTION_ACCESS_VIOLATION,
            EXCEPTION_NONCONTINUABLE,
            0,
            std::ptr::null(),
        );
    }
    std::process::abort()
}

fn terminate_for_out_of_memory() -> ! {
    // The production Job Object enforces the memory ceiling. Fault injection uses Windows' native
    // out-of-memory status directly so tests exercise the same uncatchable process-death boundary
    // without transiently committing hundreds of MiB on the host.
    const STATUS_NO_MEMORY: u32 = 0xC000_0017;
    unsafe extern "system" {
        fn TerminateProcess(process: *mut std::ffi::c_void, exit_code: u32) -> i32;
    }
    unsafe {
        TerminateProcess(GetCurrentProcess(), STATUS_NO_MEMORY);
    }
    std::process::abort()
}

#[inline(never)]
#[allow(unconditional_recursion)]
fn overflow_stack(depth: usize) -> usize {
    let marker = [depth as u8; 4096];
    std::hint::black_box(&marker);
    overflow_stack(depth.wrapping_add(1)).wrapping_add(usize::from(marker[0]))
}

fn is_app_container() -> Result<bool, String> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(format!(
            "open renderer process token: {}",
            std::io::Error::last_os_error()
        ));
    }
    let token = NonNull::new(token).expect("OpenProcessToken returned a handle");
    let mut value = 0_u32;
    let mut returned = 0_u32;
    let result = unsafe {
        GetTokenInformation(
            token.as_ptr(),
            TokenIsAppContainer,
            (&mut value as *mut u32).cast(),
            size_of::<u32>() as u32,
            &mut returned,
        )
    };
    unsafe { CloseHandle(token.as_ptr()) };
    if result == 0 {
        Err(format!(
            "query renderer AppContainer token: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(value != 0)
    }
}

fn write_raw_header(
    mut output: File,
    session: RendererSessionId,
    major: u16,
    payload_length: u32,
) -> Result<(), String> {
    let mut header = [0_u8; HEADER_LENGTH];
    header[..4].copy_from_slice(&MAGIC);
    header[4..6].copy_from_slice(&major.to_le_bytes());
    header[6..8].copy_from_slice(&PROTOCOL_MINOR.to_le_bytes());
    header[8..10].copy_from_slice(&2_u16.to_le_bytes());
    header[12..16].copy_from_slice(&payload_length.to_le_bytes());
    header[16..24].copy_from_slice(&session.get().to_le_bytes());
    header[24..32].copy_from_slice(&1_u64.to_le_bytes());
    output
        .write_all(&header)
        .map_err(|error| format!("write raw test frame: {error}"))
}

fn valid_handle(handle: HANDLE) -> bool {
    !handle.is_null() && handle != INVALID_HANDLE_VALUE
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StartupFault {
    Silent,
    WrongNonce,
    MalformedFrame,
    OversizedFrame,
    IncompatibleVersion,
}

struct ChildOptions {
    nonce: Nonce,
    session: RendererSessionId,
    test_mode: bool,
    fault: Option<StartupFault>,
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
        let nonce = Nonce::from_hex(value("--renderer-nonce")?)
            .map_err(|error| format!("parse renderer nonce: {error}"))?;
        let session = value("--renderer-session")?
            .parse::<u64>()
            .map_err(|_| "--renderer-session requires an integer".to_string())
            .and_then(|value| RendererSessionId::new(value).map_err(|error| error.to_string()))?;
        let fault = arguments
            .iter()
            .any(|argument| argument == "--renderer-startup-fault")
            .then(|| value("--renderer-startup-fault"))
            .transpose()?
            .map(|fault| match fault.as_str() {
                "silent" => Ok(StartupFault::Silent),
                "wrong-nonce" => Ok(StartupFault::WrongNonce),
                "malformed" => Ok(StartupFault::MalformedFrame),
                "oversized" => Ok(StartupFault::OversizedFrame),
                "incompatible" => Ok(StartupFault::IncompatibleVersion),
                _ => Err(format!("unknown renderer startup fault: {fault}")),
            })
            .transpose()?;
        Ok(Self {
            nonce,
            session,
            test_mode: arguments
                .iter()
                .any(|argument| argument == "--renderer-test-mode"),
            fault,
        })
    }
}
mod connection;
mod document;
