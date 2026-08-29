use super::backend;
use super::launcher::MediaStartupFault;
use crate::media_data_protocol::{MediaDataReader, MediaSourceId};
use crate::media_frame_protocol::{
    MediaFrameWriter as DecodedFrameWriter, MediaPixelFormat, MediaVideoFrameMetadata,
};
use crate::media_protocol::{
    BrowserMediaMessage, ContainmentReport, MEDIA_HEADER_LENGTH, MEDIA_PROTOCOL_MAJOR,
    MediaFrameReader, MediaFrameWriter, MediaLimits, MediaRestrictionReport, Nonce,
    WorkerMediaMessage,
};
use std::fs::File;
use std::io::Write;
use std::mem::size_of;
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

mod options;
mod startup;
mod testing;

use options::ChildOptions;
use startup::write_raw_header;

pub(super) fn run(arguments: &[String]) -> Result<(), String> {
    let options = ChildOptions::parse(arguments)?;
    let input_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let output_handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if !valid_handle(input_handle) || !valid_handle(output_handle) {
        return Err("media IPC standard handles are invalid".into());
    }
    let data_handle = options.data_handle as HANDLE;
    let frame_handle = options.frame_handle as HANDLE;
    if !valid_handle(data_handle) || !valid_handle(frame_handle) {
        return Err("media data or frame handle is invalid".into());
    }
    let input = unsafe { File::from_raw_handle(input_handle as RawHandle) };
    let output = unsafe { File::from_raw_handle(output_handle as RawHandle) };
    let data_input = unsafe { File::from_raw_handle(data_handle as RawHandle) };
    let frame_output = unsafe { File::from_raw_handle(frame_handle as RawHandle) };
    run_protocol(input, output, data_input, frame_output, options)
}

fn run_protocol(
    input: File,
    output: File,
    data_input: File,
    frame_output: File,
    options: ChildOptions,
) -> Result<(), String> {
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
    let mut data_reader = MediaDataReader::new(data_input, options.session, options.nonce);
    let mut frame_writer = DecodedFrameWriter::new(frame_output, options.session, options.nonce);
    command_loop(
        &mut reader,
        &mut writer,
        &mut data_reader,
        &mut frame_writer,
        limits,
        options.test_mode,
    )
}

fn command_loop(
    reader: &mut MediaFrameReader<File>,
    writer: &mut MediaFrameWriter<File>,
    data_reader: &mut MediaDataReader<File>,
    frame_writer: &mut DecodedFrameWriter<File>,
    limits: MediaLimits,
    test_mode: bool,
) -> Result<(), String> {
    let mut last_source_id = 0_u64;
    let mut last_frame_id = 0_u64;
    let mut pending_frame: Option<(MediaVideoFrameMetadata, Vec<u8>)> = None;
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
            BrowserMediaMessage::DecodeSource {
                request_id,
                source_id,
                frame_id,
                encoded_length,
            } => {
                if pending_frame.is_some() {
                    return Err(
                        "media worker received a decode before acknowledging its frame".into(),
                    );
                }
                let expected_source_id = last_source_id
                    .checked_add(1)
                    .ok_or_else(|| "media source generation exhausted".to_string())?;
                if source_id != expected_source_id {
                    return Err(format!(
                        "stale media source generation {source_id}; expected {expected_source_id}"
                    ));
                }
                last_source_id = source_id;
                let expected_frame_id = last_frame_id
                    .checked_add(1)
                    .ok_or_else(|| "media frame generation exhausted".to_string())?;
                if frame_id != expected_frame_id {
                    return Err(format!(
                        "stale media frame generation {frame_id}; expected {expected_frame_id}"
                    ));
                }
                last_frame_id = frame_id;
                // This slice adapts a complete source to a seekable in-memory IMFByteStream. Keep
                // admission within the resident encoded-queue budget until streaming arrives.
                if encoded_length > limits.max_encoded_queue_bytes {
                    return Err("declared media source exceeds resident worker limits".into());
                }
                let source = MediaSourceId::new(source_id).map_err(|error| error.to_string())?;
                let bytes = data_reader
                    .read_source(source, encoded_length)
                    .map_err(|error| format!("read encoded media source: {error}"))?;
                let decoded = backend::decode(&bytes, limits)?;
                let frame = MediaVideoFrameMetadata {
                    source_id,
                    frame_id,
                    timestamp_100ns: decoded.video.timestamp_100ns,
                    duration_100ns: decoded.video.duration_100ns,
                    width: decoded.report.video_width,
                    height: decoded.report.video_height,
                    stride: decoded.video.stride,
                    format: MediaPixelFormat::Nv12,
                    data_length: decoded.video.bytes.len() as u64,
                };
                frame
                    .validate()
                    .map_err(|error| format!("validate decoded video frame: {error}"))?;
                frame_writer
                    .send_frame(frame, &decoded.video.bytes)
                    .map_err(|error| format!("write decoded video frame: {error}"))?;
                pending_frame = Some((frame, decoded.video.bytes));
                writer
                    .send_worker(&WorkerMediaMessage::Decoded {
                        request_id,
                        report: decoded.report,
                        frame,
                    })
                    .map_err(|error| error.to_string())?;
            }
            BrowserMediaMessage::AcknowledgeFrame {
                source_id,
                frame_id,
            } => {
                let Some((frame, _bytes)) = pending_frame.as_ref() else {
                    return Err(
                        "media worker received a frame acknowledgement with no pending frame"
                            .into(),
                    );
                };
                if source_id != frame.source_id || frame_id != frame.frame_id {
                    return Err(format!(
                        "stale media frame acknowledgement {source_id}/{frame_id}; expected {}/{}",
                        frame.source_id, frame.frame_id
                    ));
                }
                pending_frame.take();
                writer
                    .send_worker(&WorkerMediaMessage::FrameAcknowledged {
                        source_id,
                        frame_id,
                    })
                    .map_err(|error| error.to_string())?;
            }
            BrowserMediaMessage::Test(command) if test_mode => {
                testing::handle(command, writer, frame_writer)?;
            }
            BrowserMediaMessage::Test(_) => return Err("media test command denied".into()),
            BrowserMediaMessage::Hello { .. } => {
                return Err("media worker received a duplicate Hello".into());
            }
        }
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

fn valid_handle(handle: HANDLE) -> bool {
    !handle.is_null() && handle != INVALID_HANDLE_VALUE
}
