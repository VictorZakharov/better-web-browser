use super::{CaptureProtocolError, MAX_SAMPLE_BYTES};

/// Fixed live-video ceiling keeps two buffered frames inexpensive. The decoded-media protocol's
/// 128 MiB frame cap is intentionally not reused for unbounded capture streams.
pub(crate) const MAX_VIDEO_WIDTH: u32 = 1280;
pub(crate) const MAX_VIDEO_HEIGHT: u32 = 720;
pub(crate) const MAX_AUDIO_CHANNELS: u16 = 2;
pub(crate) const MAX_AUDIO_RATE: u32 = 48_000;
pub(crate) const MAX_AUDIO_FRAMES: u32 = 960; // 20 ms at 48 kHz.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureSampleKind {
    VideoNv12,
    AudioPcm16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CaptureSample {
    pub(crate) capture_id: u64,
    pub(crate) track_id: u64,
    pub(crate) sequence: u64,
    pub(crate) timestamp_100ns: u64,
    pub(crate) kind: CaptureSampleKind,
    pub(crate) width_or_rate: u32,
    pub(crate) height_or_frames: u32,
    pub(crate) stride_or_channels: u32,
    pub(crate) bytes: Vec<u8>,
}

impl CaptureSample {
    pub(crate) fn validate(&self) -> Result<(), CaptureProtocolError> {
        if self.capture_id == 0 || self.track_id == 0 || self.sequence == 0 {
            return Err(CaptureProtocolError::InvalidPayload("sample identity"));
        }
        if self.bytes.is_empty() || self.bytes.len() > MAX_SAMPLE_BYTES {
            return Err(CaptureProtocolError::InvalidPayload("sample size"));
        }
        let expected = match self.kind {
            CaptureSampleKind::VideoNv12 => {
                let (width, height, stride) = (
                    self.width_or_rate,
                    self.height_or_frames,
                    self.stride_or_channels,
                );
                if width == 0
                    || height == 0
                    || width > MAX_VIDEO_WIDTH
                    || height > MAX_VIDEO_HEIGHT
                    || !width.is_multiple_of(2)
                    || !height.is_multiple_of(2)
                    || stride < width
                    || stride > MAX_VIDEO_WIDTH * 4
                    || !stride.is_multiple_of(2)
                {
                    return Err(CaptureProtocolError::InvalidPayload("NV12 shape"));
                }
                // MF may pad NV12 vertically. The allocation height must be exact, even, and
                // no smaller than the visible height; future renderer conversion can crop it.
                let length = self.bytes.len() as u64;
                let numerator = length * 2;
                let denominator = u64::from(stride) * 3;
                if !numerator.is_multiple_of(denominator) {
                    return Err(CaptureProtocolError::InvalidPayload("NV12 allocation"));
                }
                let storage_height = numerator / denominator;
                if storage_height < u64::from(height)
                    || storage_height > u64::from(MAX_VIDEO_HEIGHT) + 64
                    || !storage_height.is_multiple_of(2)
                {
                    return Err(CaptureProtocolError::InvalidPayload(
                        "NV12 allocation height",
                    ));
                }
                length
            }
            CaptureSampleKind::AudioPcm16 => {
                let (rate, frames, channels) = (
                    self.width_or_rate,
                    self.height_or_frames,
                    self.stride_or_channels,
                );
                if !(8_000..=MAX_AUDIO_RATE).contains(&rate)
                    || frames == 0
                    || frames > MAX_AUDIO_FRAMES
                    || channels == 0
                    || channels > u32::from(MAX_AUDIO_CHANNELS)
                    || u64::from(frames) * 1_000 > u64::from(rate) * 20
                {
                    return Err(CaptureProtocolError::InvalidPayload("PCM shape"));
                }
                u64::from(frames) * u64::from(channels) * 2
            }
        };
        if self.bytes.len() as u64 != expected {
            return Err(CaptureProtocolError::InvalidPayload("sample byte count"));
        }
        Ok(())
    }
}
