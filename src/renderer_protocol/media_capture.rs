//! Browser-authoritative live capture. The renderer names a document and request, never a device.

use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;

pub const MAX_CAPTURE_FRAME_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCaptureAction {
    Start {
        camera: bool,
        microphone: bool,
    },
    /// 0 stops both tracks; 1 camera; 2 microphone.
    Stop {
        track_id: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaCaptureRequest {
    pub document: DocumentId,
    pub request_id: u64,
    pub client: RequestClient,
    pub action: MediaCaptureAction,
}

impl MediaCaptureRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload("capture request identifier"));
        }
        if let MediaCaptureAction::Start { camera, microphone } = self.action
            && !camera
            && !microphone
        {
            return Err(ProtocolError::InvalidPayload("empty capture request"));
        }
        if let MediaCaptureAction::Stop { track_id } = self.action
            && track_id > 2
        {
            return Err(ProtocolError::InvalidPayload("capture stop track"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCaptureError {
    NotAllowed,
    NotFound,
    NotReadable,
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCaptureEvent {
    Started { camera: bool, microphone: bool },
    TrackEnded { track_id: u8 },
    Ended,
    Error(MediaCaptureError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaCaptureUpdate {
    pub document: DocumentId,
    pub request_id: u64,
    pub event: MediaCaptureEvent,
}

impl MediaCaptureUpdate {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload("capture update identifier"));
        }
        if let MediaCaptureEvent::Started { camera, microphone } = self.event
            && !camera
            && !microphone
        {
            return Err(ProtocolError::InvalidPayload("empty capture stream"));
        }
        if let MediaCaptureEvent::TrackEnded { track_id } = self.event
            && !(1..=2).contains(&track_id)
        {
            return Err(ProtocolError::InvalidPayload("capture ended track"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCaptureFrameKind {
    VideoNv12,
    AudioPcm16,
}

/// The private broker capture ID is deliberately absent. The browser must bind each sample
/// to its granted document/request before it crosses this pipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaCaptureFrame {
    pub document: DocumentId,
    pub request_id: u64,
    pub track_id: u64,
    pub sequence: u64,
    pub timestamp_100ns: u64,
    pub kind: MediaCaptureFrameKind,
    pub width_or_rate: u32,
    pub height_or_frames: u32,
    pub stride_or_channels: u32,
    pub bytes: Vec<u8>,
}

impl MediaCaptureFrame {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 || self.sequence == 0 {
            return Err(ProtocolError::InvalidPayload("capture frame identity"));
        }
        if self.bytes.is_empty() || self.bytes.len() > MAX_CAPTURE_FRAME_BYTES {
            return Err(ProtocolError::InvalidPayload("capture frame size"));
        }
        let expected = match self.kind {
            MediaCaptureFrameKind::VideoNv12 => {
                let (width, height, stride) = (
                    self.width_or_rate,
                    self.height_or_frames,
                    self.stride_or_channels,
                );
                if self.track_id != 1
                    || width == 0
                    || height == 0
                    || width > 1280
                    || height > 720
                    || !width.is_multiple_of(2)
                    || !height.is_multiple_of(2)
                    || stride < width
                    || stride > 1280 * 4
                    || !stride.is_multiple_of(2)
                {
                    return Err(ProtocolError::InvalidPayload("capture NV12 shape"));
                }
                let numerator = self.bytes.len() as u64 * 2;
                let denominator = u64::from(stride) * 3;
                if !numerator.is_multiple_of(denominator) {
                    return Err(ProtocolError::InvalidPayload("capture NV12 allocation"));
                }
                let storage_height = numerator / denominator;
                if storage_height < u64::from(height)
                    || storage_height > 720 + 64
                    || !storage_height.is_multiple_of(2)
                {
                    return Err(ProtocolError::InvalidPayload("capture NV12 storage height"));
                }
                self.bytes.len() as u64
            }
            MediaCaptureFrameKind::AudioPcm16 => {
                let (rate, frames, channels) = (
                    self.width_or_rate,
                    self.height_or_frames,
                    self.stride_or_channels,
                );
                if self.track_id != 2
                    || !(8_000..=48_000).contains(&rate)
                    || frames == 0
                    || frames > 960
                    || !(1..=2).contains(&channels)
                    || u64::from(frames) * 1_000 > u64::from(rate) * 20
                {
                    return Err(ProtocolError::InvalidPayload("capture PCM shape"));
                }
                u64::from(frames) * u64::from(channels) * 2
            }
        };
        if self.bytes.len() as u64 != expected {
            return Err(ProtocolError::InvalidPayload("capture frame byte count"));
        }
        Ok(())
    }
}
