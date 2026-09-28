//! Private browser-controlled capture worker protocol.
//!
//! Camera and microphone access is deliberately unavailable to the renderer and the existing
//! decoder worker. A capture process is launched for one browser grant, and only the browser owns
//! its control pipe. Samples use an independently bounded, one-way pipe.

mod codec;
mod sample;

pub(crate) use codec::{CaptureFrameReader, CaptureFrameWriter, CaptureProtocolError};
pub(crate) use sample::{CaptureSample, CaptureSampleKind};

use crate::renderer_protocol::Nonce;

pub(crate) const MAGIC: [u8; 4] = *b"BRC1";
pub(crate) const HEADER_LENGTH: usize = 32;
pub(crate) const PROTOCOL_MAJOR: u16 = 1;
pub(crate) const PROTOCOL_MINOR: u16 = 0;
pub(crate) const MAX_CONTROL_BYTES: usize = 64;
pub(crate) const MAX_SAMPLE_BYTES: usize = 2 * 1024 * 1024;

/// The exact AppContainer capabilities for a single, browser-authorized grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CaptureDevices {
    pub(crate) camera: bool,
    pub(crate) microphone: bool,
}

impl CaptureDevices {
    pub(crate) fn validate(self) -> Result<(), CaptureProtocolError> {
        if !self.camera && !self.microphone {
            return Err(CaptureProtocolError::InvalidPayload("empty device grant"));
        }
        Ok(())
    }

    pub(crate) const fn bits(self) -> u8 {
        (self.camera as u8) | ((self.microphone as u8) << 1)
    }

    pub(crate) fn from_bits(bits: u8) -> Result<Self, CaptureProtocolError> {
        if bits == 0 || bits & !3 != 0 {
            return Err(CaptureProtocolError::InvalidPayload("device grant bits"));
        }
        Ok(Self {
            camera: bits & 1 != 0,
            microphone: bits & 2 != 0,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CaptureSessionId(u64);

impl CaptureSessionId {
    pub(crate) fn new(value: u64) -> Result<Self, CaptureProtocolError> {
        if value == 0 {
            return Err(CaptureProtocolError::InvalidPayload("capture session"));
        }
        Ok(Self(value))
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureFailure {
    NoDevice,
    AccessDenied,
    DeviceBusy,
    InvalidFormat,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BrowserCaptureMessage {
    Hello {
        nonce: Nonce,
        devices: CaptureDevices,
    },
    Start {
        capture_id: u64,
    },
    Stop {
        capture_id: u64,
    },
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkerCaptureMessage {
    Ready {
        nonce: Nonce,
    },
    Started {
        capture_id: u64,
    },
    Stopped {
        capture_id: u64,
    },
    Failed {
        capture_id: u64,
        reason: CaptureFailure,
    },
    ShutdownComplete,
}

#[cfg(test)]
mod tests;
