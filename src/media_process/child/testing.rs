use super::probe_restrictions;
use crate::media_frame_protocol::{
    MediaFrameTestFault, MediaFrameWriter as DecodedFrameWriter, MediaPixelFormat,
    MediaVideoFrameMetadata,
};
use crate::media_protocol::{
    MEDIA_HEADER_LENGTH, MediaFrameWriter, MediaTestCommand, WorkerMediaMessage,
};
use std::fs::File;
use std::io::Write;
use std::time::Duration;

pub(super) fn handle(
    command: MediaTestCommand,
    writer: &mut MediaFrameWriter<File>,
    frame_writer: &mut DecodedFrameWriter<File>,
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
        MediaTestCommand::WriteMalformedDecodedFrame => {
            write_frame_fault(frame_writer, MediaFrameTestFault::Malformed)
        }
        MediaTestCommand::WriteTruncatedDecodedFrame => {
            write_frame_fault(frame_writer, MediaFrameTestFault::Truncated)
        }
        MediaTestCommand::WriteOversizedDecodedFrame => {
            write_frame_fault(frame_writer, MediaFrameTestFault::Oversized)
        }
    }
}

fn write_frame_fault(
    writer: &mut DecodedFrameWriter<File>,
    fault: MediaFrameTestFault,
) -> Result<(), String> {
    writer
        .write_fault_for_test(
            MediaVideoFrameMetadata {
                source_id: 1,
                frame_id: 1,
                timestamp_100ns: 0,
                duration_100ns: 333_333,
                width: 2,
                height: 2,
                stride: 2,
                format: MediaPixelFormat::Nv12,
                data_length: 6,
            },
            fault,
        )
        .map_err(|error| format!("write invalid decoded frame: {error}"))?;
    Err("injected invalid decoded frame".into())
}
