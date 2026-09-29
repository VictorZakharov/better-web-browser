//! Private, browser-controlled device capture process.
//!
//! No renderer handle or JavaScript binding is connected to this module. The process and its
//! hardware capabilities are created only after browser-owned permission admission. Tests use a
//! deterministic provider and never open devices.

mod broker;
mod child;
pub(crate) mod launcher;
mod native;

use std::path::Path;

/// Browser-owned device grant. This is not a web-facing capability; only the trusted browser
/// process may construct it after checking the current document, origin, foreground state, and
/// user permission. A session is one-shot and confined to its own AppContainer capture child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureGrant {
    pub camera: bool,
    pub microphone: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturedSampleKind {
    VideoNv12,
    AudioPcm16,
}

/// A validated sample copied out of the contained capture process. Video is bounded to
/// 1280×720 NV12 and audio to 20 ms of 48-kHz stereo PCM16 by the private wire protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedSample {
    pub capture_id: u64,
    pub track_id: u64,
    pub sequence: u64,
    pub timestamp_100ns: u64,
    pub kind: CapturedSampleKind,
    pub width_or_rate: u32,
    pub height_or_frames: u32,
    pub stride_or_channels: u32,
    pub bytes: Vec<u8>,
}

impl From<crate::capture_protocol::CaptureSample> for CapturedSample {
    fn from(sample: crate::capture_protocol::CaptureSample) -> Self {
        use crate::capture_protocol::CaptureSampleKind;
        Self {
            capture_id: sample.capture_id,
            track_id: sample.track_id,
            sequence: sample.sequence,
            timestamp_100ns: sample.timestamp_100ns,
            kind: match sample.kind {
                CaptureSampleKind::VideoNv12 => CapturedSampleKind::VideoNv12,
                CaptureSampleKind::AudioPcm16 => CapturedSampleKind::AudioPcm16,
            },
            width_or_rate: sample.width_or_rate,
            height_or_frames: sample.height_or_frames,
            stride_or_channels: sample.stride_or_channels,
            bytes: sample.bytes,
        }
    }
}

/// Owned only by a browser broker thread. Dropping a session revokes the contained Job without
/// waiting for a potentially blocked native device reader; never drop one on the UI thread.
pub struct CaptureSession(broker::CaptureSession);

#[derive(Debug)]
pub enum CaptureStartError {
    NoDevice,
    AccessDenied,
    NotReadable(String),
}

impl CaptureSession {
    pub fn launch(executable: &Path, grant: CaptureGrant) -> Result<Self, String> {
        let devices = crate::capture_protocol::CaptureDevices {
            camera: grant.camera,
            microphone: grant.microphone,
        };
        devices.validate().map_err(|error| error.to_string())?;
        broker::CaptureSession::launch(&launcher::CaptureLaunchOptions::new(executable, devices))
            .map(Self)
    }

    pub fn start(&mut self, capture_id: u64) -> Result<(), CaptureStartError> {
        self.0.start(capture_id)
    }

    pub fn latest_video(&self) -> Result<Option<CapturedSample>, String> {
        self.0.latest_video().map(|sample| sample.map(Into::into))
    }

    pub fn next_audio(&self) -> Result<Option<CapturedSample>, String> {
        self.0.next_audio().map(|sample| sample.map(Into::into))
    }
}

/// Internal child role, dispatched before any interactive browser initialization.
pub fn run_child_from_args(arguments: &[String]) -> Option<Result<(), String>> {
    arguments
        .iter()
        .any(|argument| argument == "--capture-process")
        .then(|| child::run(arguments))
}
