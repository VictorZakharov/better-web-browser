//! Bootstrap of the direct renderer-to-media-worker channels.

use crate::limits::{MEDIA_COMMAND_TIMEOUT, MEDIA_STARTUP_TIMEOUT};
use crate::media_process::{MediaClient, MediaClientEndpoints};
use crate::media_protocol::{MediaSessionId, Nonce};
use std::fs::File;
use std::os::windows::io::{FromRawHandle, RawHandle};
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};

pub(super) struct ChildMediaOptions {
    nonce: Nonce,
    session: MediaSessionId,
    control_input: usize,
    control_output: usize,
    data_output: usize,
    frame_input: usize,
}

impl ChildMediaOptions {
    pub(super) fn parse(arguments: &[String]) -> Result<Option<Self>, String> {
        let enabled = arguments
            .iter()
            .any(|argument| argument == "--renderer-media-nonce");
        if !enabled {
            return Ok(None);
        }
        let value = |name: &str| {
            arguments
                .iter()
                .position(|argument| argument == name)
                .and_then(|index| arguments.get(index + 1))
                .ok_or_else(|| format!("{name} requires a value"))
        };
        let handle = |name: &str| {
            value(name)?
                .parse::<usize>()
                .map_err(|_| format!("{name} requires a handle value"))
        };
        Ok(Some(Self {
            nonce: Nonce::from_hex(value("--renderer-media-nonce")?)
                .map_err(|error| format!("parse renderer media nonce: {error}"))?,
            session: value("--renderer-media-session")?
                .parse::<u64>()
                .map_err(|_| "--renderer-media-session requires an integer".to_string())
                .and_then(|value| MediaSessionId::new(value).map_err(|error| error.to_string()))?,
            control_input: handle("--renderer-media-control-input")?,
            control_output: handle("--renderer-media-control-output")?,
            data_output: handle("--renderer-media-data-output")?,
            frame_input: handle("--renderer-media-frame-input")?,
        }))
    }

    pub(super) fn connect(self) -> Result<MediaClient, String> {
        for handle in [
            self.control_input,
            self.control_output,
            self.data_output,
            self.frame_input,
        ] {
            if !valid_handle(handle as HANDLE) {
                return Err("renderer inherited an invalid media handle".into());
            }
        }
        let endpoints = unsafe {
            MediaClientEndpoints {
                control_input: File::from_raw_handle(self.control_input as RawHandle),
                control_output: File::from_raw_handle(self.control_output as RawHandle),
                data_output: File::from_raw_handle(self.data_output as RawHandle),
                frame_input: File::from_raw_handle(self.frame_input as RawHandle),
            }
        };
        MediaClient::connect(
            endpoints,
            self.session,
            self.nonce,
            MEDIA_STARTUP_TIMEOUT,
            MEDIA_COMMAND_TIMEOUT,
        )
    }
}

fn valid_handle(handle: HANDLE) -> bool {
    !handle.is_null() && handle != INVALID_HANDLE_VALUE
}
