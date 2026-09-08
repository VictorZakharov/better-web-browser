use super::{MediaCodecFamily, MediaLimits, MediaProtocolError};
use crate::limits::{
    MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES, MAX_MEDIA_DECODED_SAMPLES, MAX_MEDIA_DURATION_100NS,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaDecodeReport {
    pub buffered: super::MediaBufferedExtent,
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
        self.buffered.validate()?;
        if self.buffered.video_end_100ns == 0
            || self.buffered.audio_end_100ns == 0
            || self.buffered.end_100ns() > self.duration_100ns
        {
            return Err(MediaProtocolError::InvalidPayload(
                "decoded buffered extents",
            ));
        }
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
        // These counters describe cumulative streamed output, not resident allocations. Video is
        // pull-decoded one frame at a time and audio samples are consumed by the bounded output
        // queue. Bound each unit and the independently checked sample counts instead of rejecting
        // ordinary high-frame-rate segments because their theoretical full decode exceeds RAM.
        let maximum_video_bytes = limits
            .max_decoded_frame_bytes
            .checked_mul(u64::from(self.video_samples))
            .ok_or(MediaProtocolError::InvalidPayload(
                "decoded video byte count",
            ))?;
        let maximum_audio_bytes = (MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES as u64)
            .checked_mul(u64::from(self.audio_samples))
            .ok_or(MediaProtocolError::InvalidPayload(
                "decoded audio byte count",
            ))?;
        if self.video_decoded_bytes == 0 || self.video_decoded_bytes > maximum_video_bytes {
            return Err(MediaProtocolError::InvalidPayload("decoded byte count"));
        }
        if self.audio_decoded_bytes == 0 || self.audio_decoded_bytes > maximum_audio_bytes {
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
