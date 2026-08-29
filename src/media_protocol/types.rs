use super::{ContainmentReport, MediaProtocolError, Nonce};
use crate::limits::{
    MAX_MEDIA_CONTROL_PAYLOAD, MAX_MEDIA_DECODED_FRAME_BYTES, MAX_MEDIA_DECODED_FRAMES,
    MAX_MEDIA_DECODED_SAMPLES, MAX_MEDIA_DECODED_SOURCE_BYTES, MAX_MEDIA_DECODER_CANDIDATES,
    MAX_MEDIA_DIMENSION, MAX_MEDIA_DURATION_100NS, MAX_MEDIA_ENCODED_BYTES,
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
#[repr(u16)]
pub enum MediaCodecFamily {
    H264 = 1,
    AacLc = 2,
}

impl MediaCodecFamily {
    pub(crate) const fn wire_code(self) -> u16 {
        self as u16
    }

    pub(crate) fn from_wire(value: u16) -> Result<Self, MediaProtocolError> {
        match value {
            1 => Ok(Self::H264),
            2 => Ok(Self::AacLc),
            _ => Err(MediaProtocolError::InvalidPayload("codec family")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaDecodeReport {
    pub encoded_bytes: u64,
    pub video_codec: MediaCodecFamily,
    pub audio_codec: MediaCodecFamily,
    pub source_reader_hresult: i32,
    pub video_decode_hresult: i32,
    pub audio_decode_hresult: i32,
    pub video_width: u32,
    pub video_height: u32,
    pub audio_sample_rate: u32,
    pub audio_channels: u16,
    pub video_samples: u32,
    pub audio_samples: u32,
    pub video_decoded_bytes: u64,
    pub audio_decoded_bytes: u64,
    pub video_first_timestamp_100ns: i64,
    pub video_last_timestamp_100ns: i64,
    pub audio_first_timestamp_100ns: i64,
    pub audio_last_timestamp_100ns: i64,
    pub duration_100ns: u64,
    pub decode_micros: u64,
}

impl MediaDecodeReport {
    pub fn validate(self, limits: MediaLimits) -> Result<(), MediaProtocolError> {
        limits.validate()?;
        if self.encoded_bytes == 0 || self.encoded_bytes > limits.max_encoded_bytes {
            return Err(MediaProtocolError::InvalidPayload("decoded source length"));
        }
        if self.video_codec != MediaCodecFamily::H264 || self.audio_codec != MediaCodecFamily::AacLc
        {
            return Err(MediaProtocolError::InvalidPayload("decoded codec family"));
        }
        if self.source_reader_hresult < 0
            || self.video_decode_hresult < 0
            || self.audio_decode_hresult < 0
        {
            return Err(MediaProtocolError::InvalidPayload("decode HRESULT"));
        }
        if self.video_width == 0
            || self.video_height == 0
            || self.video_width > limits.max_dimension
            || self.video_height > limits.max_dimension
        {
            return Err(MediaProtocolError::InvalidPayload(
                "decoded video dimensions",
            ));
        }
        if self.audio_sample_rate == 0
            || self.audio_sample_rate > 384_000
            || self.audio_channels == 0
            || self.audio_channels > 32
        {
            return Err(MediaProtocolError::InvalidPayload("decoded audio format"));
        }
        if self.video_samples == 0
            || self.audio_samples == 0
            || self.video_samples as usize > MAX_MEDIA_DECODED_SAMPLES
            || self.audio_samples as usize > MAX_MEDIA_DECODED_SAMPLES
        {
            return Err(MediaProtocolError::InvalidPayload("decoded sample count"));
        }
        let decoded_bytes = self
            .video_decoded_bytes
            .checked_add(self.audio_decoded_bytes)
            .ok_or(MediaProtocolError::InvalidPayload("decoded byte count"))?;
        if decoded_bytes == 0 || decoded_bytes > MAX_MEDIA_DECODED_SOURCE_BYTES {
            return Err(MediaProtocolError::InvalidPayload("decoded byte count"));
        }
        if self.duration_100ns == 0 || self.duration_100ns > MAX_MEDIA_DURATION_100NS {
            return Err(MediaProtocolError::InvalidPayload("decoded duration"));
        }
        for (first, last) in [
            (
                self.video_first_timestamp_100ns,
                self.video_last_timestamp_100ns,
            ),
            (
                self.audio_first_timestamp_100ns,
                self.audio_last_timestamp_100ns,
            ),
        ] {
            if first > last
                || first.unsigned_abs() > MAX_MEDIA_DURATION_100NS
                || last.unsigned_abs() > MAX_MEDIA_DURATION_100NS
            {
                return Err(MediaProtocolError::InvalidPayload(
                    "decoded timestamp bounds",
                ));
            }
        }
        let maximum_decode_micros = u64::from(limits.probe_timeout_millis) * 1_000;
        if self.decode_micros > maximum_decode_micros {
            return Err(MediaProtocolError::InvalidPayload("decode duration"));
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
    WriteMalformedDecodedFrame,
    WriteTruncatedDecodedFrame,
    WriteOversizedDecodedFrame,
    ProbeRestrictions { loopback_port: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserMediaMessage {
    Hello {
        nonce: Nonce,
        limits: MediaLimits,
    },
    Ping(u64),
    Shutdown,
    Probe {
        request_id: u64,
    },
    DecodeSource {
        request_id: u64,
        source_id: u64,
        frame_id: u64,
        encoded_length: u64,
    },
    AcknowledgeFrame {
        source_id: u64,
        frame_id: u64,
    },
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
    Decoded {
        request_id: u64,
        report: MediaDecodeReport,
        frame: crate::media_frame_protocol::MediaVideoFrameMetadata,
    },
    FrameAcknowledged {
        source_id: u64,
        frame_id: u64,
    },
    Restrictions(MediaRestrictionReport),
}
