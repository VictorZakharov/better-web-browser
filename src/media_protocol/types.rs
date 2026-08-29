use super::{ContainmentReport, MediaProtocolError, Nonce};
use crate::limits::{
    MAX_MEDIA_CONTROL_PAYLOAD, MAX_MEDIA_DECODED_FRAME_BYTES, MAX_MEDIA_DECODED_FRAMES,
    MAX_MEDIA_DECODER_CANDIDATES, MAX_MEDIA_DIMENSION, MAX_MEDIA_ENCODED_BYTES,
    MAX_MEDIA_ENCODED_QUEUE_BYTES, MAX_MEDIA_TRACKS, MEDIA_COMMAND_TIMEOUT,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaSessionId(u64);

impl MediaSessionId {
    pub fn new(value: u64) -> Result<Self, MediaProtocolError> {
        (value != 0)
            .then_some(Self(value))
            .ok_or(MediaProtocolError::InvalidPayload("zero media session"))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaLimits {
    pub max_control_payload: u32,
    pub max_tracks: u16,
    pub max_dimension: u32,
    pub max_encoded_bytes: u64,
    pub max_encoded_queue_bytes: u64,
    pub max_decoded_frame_bytes: u64,
    pub max_decoded_frames: u16,
    pub max_decoder_candidates: u16,
    pub probe_timeout_millis: u32,
}

impl Default for MediaLimits {
    fn default() -> Self {
        Self {
            max_control_payload: MAX_MEDIA_CONTROL_PAYLOAD as u32,
            max_tracks: MAX_MEDIA_TRACKS as u16,
            max_dimension: MAX_MEDIA_DIMENSION,
            max_encoded_bytes: MAX_MEDIA_ENCODED_BYTES as u64,
            max_encoded_queue_bytes: MAX_MEDIA_ENCODED_QUEUE_BYTES as u64,
            max_decoded_frame_bytes: MAX_MEDIA_DECODED_FRAME_BYTES as u64,
            max_decoded_frames: MAX_MEDIA_DECODED_FRAMES as u16,
            max_decoder_candidates: MAX_MEDIA_DECODER_CANDIDATES as u16,
            probe_timeout_millis: MEDIA_COMMAND_TIMEOUT.as_millis() as u32,
        }
    }
}

impl MediaLimits {
    pub fn validate(self) -> Result<(), MediaProtocolError> {
        require_bounded(
            self.max_control_payload,
            MAX_MEDIA_CONTROL_PAYLOAD as u32,
            "control payload",
        )?;
        require_bounded(self.max_tracks, MAX_MEDIA_TRACKS as u16, "track count")?;
        require_bounded(self.max_dimension, MAX_MEDIA_DIMENSION, "media dimension")?;
        require_bounded(
            self.max_encoded_bytes,
            MAX_MEDIA_ENCODED_BYTES as u64,
            "encoded bytes",
        )?;
        require_bounded(
            self.max_encoded_queue_bytes,
            MAX_MEDIA_ENCODED_QUEUE_BYTES as u64,
            "encoded queue bytes",
        )?;
        require_bounded(
            self.max_decoded_frame_bytes,
            MAX_MEDIA_DECODED_FRAME_BYTES as u64,
            "decoded frame bytes",
        )?;
        require_bounded(
            self.max_decoded_frames,
            MAX_MEDIA_DECODED_FRAMES as u16,
            "decoded frame count",
        )?;
        require_bounded(
            self.max_decoder_candidates,
            MAX_MEDIA_DECODER_CANDIDATES as u16,
            "decoder candidates",
        )?;
        require_bounded(
            self.probe_timeout_millis,
            MEDIA_COMMAND_TIMEOUT.as_millis() as u32,
            "probe timeout",
        )?;
        if self.max_encoded_queue_bytes > self.max_encoded_bytes {
            return Err(MediaProtocolError::InvalidPayload(
                "encoded queue exceeds total encoded bytes",
            ));
        }
        Ok(())
    }
}

fn require_bounded<T>(value: T, maximum: T, field: &'static str) -> Result<(), MediaProtocolError>
where
    T: Copy + Default + Ord,
{
    (value > T::default() && value <= maximum)
        .then_some(())
        .ok_or(MediaProtocolError::InvalidPayload(field))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaCapabilityReport {
    pub startup_hresult: i32,
    pub h264_hresult: i32,
    pub aac_hresult: i32,
    pub h264_decoders: u16,
    pub aac_decoders: u16,
    pub probe_micros: u64,
}

impl MediaCapabilityReport {
    pub fn validate(self, limits: MediaLimits) -> Result<(), MediaProtocolError> {
        limits.validate()?;
        if self.h264_decoders > limits.max_decoder_candidates
            || self.aac_decoders > limits.max_decoder_candidates
        {
            return Err(MediaProtocolError::InvalidPayload("decoder count"));
        }
        if self.startup_hresult < 0 && (self.h264_decoders != 0 || self.aac_decoders != 0) {
            return Err(MediaProtocolError::InvalidPayload(
                "decoders reported after backend startup failure",
            ));
        }
        if self.h264_hresult < 0 && self.h264_decoders != 0 {
            return Err(MediaProtocolError::InvalidPayload(
                "H.264 decoders reported after enumeration failure",
            ));
        }
        if self.aac_hresult < 0 && self.aac_decoders != 0 {
            return Err(MediaProtocolError::InvalidPayload(
                "AAC decoders reported after enumeration failure",
            ));
        }
        let maximum_probe_micros = u64::from(limits.probe_timeout_millis) * 1_000;
        if self.probe_micros > maximum_probe_micros {
            return Err(MediaProtocolError::InvalidPayload("probe duration"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaRestrictionReport {
    pub child_launch_denied: bool,
    pub loopback_denied: bool,
    pub internet_denied: bool,
    pub child_error: i32,
    pub loopback_error: i32,
    pub internet_error: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaTestCommand {
    Crash,
    Hang,
    DelayResponse { millis: u16 },
    WriteMalformedFrame,
    ProbeRestrictions { loopback_port: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserMediaMessage {
    Hello { nonce: Nonce, limits: MediaLimits },
    Ping(u64),
    Shutdown,
    Probe { request_id: u64 },
    Test(MediaTestCommand),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerMediaMessage {
    Ready {
        nonce: Nonce,
        containment: ContainmentReport,
    },
    Pong(u64),
    ShutdownComplete,
    Capability {
        request_id: u64,
        report: MediaCapabilityReport,
    },
    Restrictions(MediaRestrictionReport),
}
