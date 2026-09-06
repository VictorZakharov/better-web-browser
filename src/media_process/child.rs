use super::backend;
use super::launcher::MediaStartupFault;
use crate::media_data_protocol::MediaDataReader;
use crate::media_frame_protocol::MediaFrameWriter as DecodedFrameWriter;
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

mod audio;
mod containment;
use containment::*;
mod options;
mod playback;
mod startup;
mod testing;

use options::ChildOptions;
use playback::Playback;
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
        options.test_mode || options.silent_audio,
    )
}

fn command_loop(
    reader: &mut MediaFrameReader<File>,
    writer: &mut MediaFrameWriter<File>,
    data_reader: &mut MediaDataReader<File>,
    frame_writer: &mut DecodedFrameWriter<File>,
    limits: MediaLimits,
    test_mode: bool,
    silent_audio: bool,
) -> Result<(), String> {
    let mut playback = Playback::new(silent_audio);
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
                if let Err(error) = playback.decode_source(
                    request_id,
                    source_id,
                    frame_id,
                    encoded_length,
                    data_reader,
                    frame_writer,
                    writer,
                    limits,
                ) {
                    writer
                        .send_worker(&WorkerMediaMessage::DecodeFailed {
                            request_id,
                            error: bounded_media_failure(error),
                        })
                        .map_err(|error| error.to_string())?;
                }
            }
            BrowserMediaMessage::DecodeTracks {
                request_id,
                video_source_id,
                audio_source_id,
                frame_id,
                video_length,
                audio_length,
            } => {
                if let Err(error) = playback.decode_tracks(
                    request_id,
                    video_source_id,
                    audio_source_id,
                    frame_id,
                    video_length,
                    audio_length,
                    data_reader,
                    frame_writer,
                    writer,
                    limits,
                ) {
                    writer
                        .send_worker(&WorkerMediaMessage::DecodeFailed {
                            request_id,
                            error: bounded_media_failure(error),
                        })
                        .map_err(|error| error.to_string())?;
                }
            }
            BrowserMediaMessage::AppendTracks {
                request_id,
                source_id,
                video_source_id,
                audio_source_id,
                video_length,
                audio_length,
            } => match playback.append_tracks(
                source_id,
                video_source_id,
                audio_source_id,
                video_length,
                audio_length,
                data_reader,
                limits,
            ) {
                Ok((encoded_bytes, duration_100ns, buffered)) => writer
                    .send_worker(&WorkerMediaMessage::Appended {
                        buffered,
                        request_id,
                        source_id,
                        encoded_bytes,
                        duration_100ns,
                    })
                    .map_err(|error| error.to_string())?,
                Err(error) => writer
                    .send_worker(&WorkerMediaMessage::DecodeFailed {
                        request_id,
                        error: bounded_media_failure(error),
                    })
                    .map_err(|error| error.to_string())?,
            },
            BrowserMediaMessage::AcknowledgeFrame {
                source_id,
                frame_id,
            } => {
                playback.acknowledge(source_id, frame_id, writer)?;
            }
            BrowserMediaMessage::RequestFrame {
                source_id,
                frame_id,
            } => {
                if let Err(error) =
                    playback.request_frame(source_id, frame_id, frame_writer, writer)
                {
                    let _ = writer.send_worker(&WorkerMediaMessage::DecodeFailed {
                        request_id: frame_id,
                        error: bounded_media_failure(error.clone()),
                    });
                    return Err(error);
                }
            }
            BrowserMediaMessage::SetPlayback {
                source_id,
                playing,
                volume_millis,
            } => {
                let state = playback.set_playback(source_id, playing, volume_millis)?;
                writer
                    .send_worker(&WorkerMediaMessage::PlaybackState(state))
                    .map_err(|error| error.to_string())?;
            }
            BrowserMediaMessage::PlaybackState { source_id } => {
                let state = playback.playback_state(source_id)?;
                writer
                    .send_worker(&WorkerMediaMessage::PlaybackState(state))
                    .map_err(|error| error.to_string())?;
            }
            BrowserMediaMessage::SeekPlayback {
                source_id,
                position_100ns,
            } => {
                let state = playback.seek(source_id, position_100ns)?;
                writer
                    .send_worker(&WorkerMediaMessage::PlaybackState(state))
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

fn bounded_media_failure(mut error: String) -> String {
    while error.len() > crate::limits::MAX_MEDIA_FAILURE_BYTES {
        error.pop();
    }
    if error.is_empty() {
        "media decode failed".into()
    } else {
        error
    }
}

fn valid_handle(handle: HANDLE) -> bool {
    !handle.is_null() && handle != INVALID_HANDLE_VALUE
}
