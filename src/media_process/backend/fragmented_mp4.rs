//! Bounded fragmented-MP4 parsing for adaptive H.264 playback.
//!
//! Media Foundation's Source Reader does not consistently expose video samples from the
//! fragmented streams served by YouTube. This module owns only the ISO-BMFF bridge needed before
//! feeding length-prefixed AVC samples to the native H.264 transform.

use crate::limits::{MAX_MEDIA_DECODED_SAMPLES, MAX_MEDIA_DURATION_100NS};
use crate::media_protocol::MediaLimits;

mod boxes;
mod fragment;
mod init;

const TICKS_PER_SECOND: u64 = 10_000_000;

#[derive(Clone, Debug)]
pub(super) struct VideoTrack {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) nal_length_size: usize,
    pub(super) sequence_header: Vec<u8>,
    pub(super) samples: Vec<VideoSample>,
}

#[derive(Clone, Debug)]
pub(super) struct VideoSample {
    pub(super) bytes: Vec<u8>,
    pub(super) timestamp_100ns: i64,
    pub(super) duration_100ns: u64,
    pub(super) key_frame: bool,
}

impl VideoTrack {
    pub(super) fn duration_100ns(&self) -> u64 {
        self.samples
            .iter()
            .map(|sample| {
                sample
                    .timestamp_100ns
                    .max(0)
                    .saturating_add(sample.duration_100ns as i64) as u64
            })
            .max()
            .unwrap_or(0)
    }

    pub(super) fn decoded_bytes(&self) -> Result<u64, String> {
        let frame_bytes = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .and_then(|pixels| pixels.checked_mul(3))
            .map(|bytes| bytes / 2)
            .ok_or_else(|| "decoded H.264 dimensions overflowed".to_string())?;
        frame_bytes
            .checked_mul(self.samples.len() as u64)
            .ok_or_else(|| "decoded H.264 byte count overflowed".to_string())
    }
}

pub(super) fn parse_video(bytes: &[u8], limits: MediaLimits) -> Result<VideoTrack, String> {
    let top = boxes::parse(bytes, 0)?;
    let moov = top
        .iter()
        .find(|entry| entry.kind == *b"moov")
        .ok_or_else(|| "fragmented MP4 has no movie box".to_string())?;
    let metadata = init::parse(*moov)?;
    if metadata.width == 0
        || metadata.height == 0
        || metadata.width > limits.max_dimension
        || metadata.height > limits.max_dimension
    {
        return Err("fragmented MP4 dimensions exceed worker limits".into());
    }
    let mut samples = Vec::new();
    for entry in top.iter().filter(|entry| entry.kind == *b"moof") {
        fragment::parse(bytes, *entry, &metadata, &mut samples)?;
        if samples.len() > MAX_MEDIA_DECODED_SAMPLES {
            return Err("fragmented MP4 sample count exceeds worker limits".into());
        }
    }
    if samples.is_empty() {
        return Err("fragmented MP4 contains no H.264 samples".into());
    }
    let track = VideoTrack {
        width: metadata.width,
        height: metadata.height,
        nal_length_size: metadata.nal_length_size,
        sequence_header: metadata.sequence_header,
        samples,
    };
    if track.duration_100ns() == 0 || track.duration_100ns() > MAX_MEDIA_DURATION_100NS {
        return Err("fragmented MP4 duration exceeds worker limits".into());
    }
    Ok(track)
}

pub(super) fn annex_b_sample(sample: &[u8], length_size: usize) -> Result<Vec<u8>, String> {
    if !(1..=4).contains(&length_size) {
        return Err("AVC NAL length size is invalid".into());
    }
    let mut offset = 0_usize;
    let mut output = Vec::with_capacity(sample.len().saturating_add(16));
    while offset < sample.len() {
        if sample.len() - offset < length_size {
            return Err("AVC sample ended inside a NAL length".into());
        }
        let mut length = 0_usize;
        for byte in &sample[offset..offset + length_size] {
            length = length
                .checked_mul(256)
                .and_then(|value| value.checked_add(*byte as usize))
                .ok_or_else(|| "AVC NAL length overflowed".to_string())?;
        }
        offset += length_size;
        if length == 0 || length > sample.len() - offset {
            return Err("AVC sample declared an invalid NAL length".into());
        }
        output.extend_from_slice(&[0, 0, 0, 1]);
        output.extend_from_slice(&sample[offset..offset + length]);
        offset += length;
    }
    Ok(output)
}

pub(super) fn summary(bytes: &[u8]) -> String {
    boxes::summary(bytes)
}

fn add_signed(base: u64, offset: i64) -> Result<u64, String> {
    if offset >= 0 {
        base.checked_add(offset as u64)
    } else {
        base.checked_sub(offset.unsigned_abs())
    }
    .ok_or_else(|| "fragmented MP4 signed offset overflowed".into())
}

fn scale(value: u64, timescale: u32) -> Result<u64, String> {
    value
        .checked_mul(TICKS_PER_SECOND)
        .map(|ticks| ticks / u64::from(timescale))
        .ok_or_else(|| "fragmented MP4 timestamp overflowed".into())
}

fn scale_signed(value: u64, timescale: u32) -> Result<i64, String> {
    i64::try_from(scale(value, timescale)?)
        .map_err(|_| "fragmented MP4 timestamp is not representable".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_length_prefixed_avc_to_annex_b() {
        assert_eq!(
            annex_b_sample(&[0, 0, 0, 2, 0x65, 1, 0, 0, 0, 1, 0x41], 4).unwrap(),
            [0, 0, 0, 1, 0x65, 1, 0, 0, 0, 1, 0x41]
        );
    }

    #[test]
    fn rejects_truncated_length_prefixed_avc() {
        assert!(annex_b_sample(&[0, 0, 0, 5, 0x65], 4).is_err());
    }
}
