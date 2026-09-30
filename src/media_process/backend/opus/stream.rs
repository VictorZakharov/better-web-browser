use crate::limits::{
    MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES, MAX_MEDIA_DECODED_SAMPLES,
    MAX_MEDIA_DECODED_SOURCE_BYTES, MAX_MEDIA_DURATION_100NS,
};
use crate::opus_audio::{Limits, SAMPLE_RATE, Stream};
use std::sync::Arc;
use std::time::Instant;

pub(super) struct PcmStream {
    core: Stream,
    pub(super) channels: u16,
    pub(super) frames: u64,
    pub(super) samples: u32,
    pub(super) decoded_bytes: u64,
    pub(super) last_timestamp_100ns: i64,
}

impl PcmStream {
    pub(super) fn open(source: Arc<[u8]>, deadline: Instant) -> Result<Self, String> {
        let core = Stream::open(source, Limits::default(), None, deadline)?;
        let channels = core.channels();
        Ok(Self {
            core,
            channels,
            frames: 0,
            samples: 0,
            decoded_bytes: 0,
            last_timestamp_100ns: 0,
        })
    }

    pub(super) fn next_sample(&mut self, deadline: Instant) -> Result<Option<Vec<u8>>, String> {
        let Some(samples) = self.core.next_pcm(None, deadline)? else {
            if self.frames != self.core.frames() {
                return Err("Ogg/Opus PCM disagreed with its presentation duration".into());
            }
            return Ok(None);
        };
        let bytes = pcm16_bytes(&samples, self.channels)?;
        self.samples = self
            .samples
            .checked_add(1)
            .filter(|samples| *samples as usize <= MAX_MEDIA_DECODED_SAMPLES)
            .ok_or("Ogg/Opus playback sample count exceeds worker limit")?;
        self.decoded_bytes = self
            .decoded_bytes
            .checked_add(bytes.len() as u64)
            .filter(|bytes| *bytes <= MAX_MEDIA_DECODED_SOURCE_BYTES)
            .ok_or("Ogg/Opus decoded bytes exceed worker limit")?;
        self.last_timestamp_100ns = i64::try_from(frames_to_100ns(self.frames)?)
            .map_err(|_| "Ogg/Opus PCM timestamp overflow")?;
        self.frames = self
            .frames
            .checked_add(samples.len() as u64 / u64::from(self.channels))
            .filter(|frames| *frames <= self.core.frames())
            .ok_or("Ogg/Opus PCM exceeded its presentation duration")?;
        frames_to_100ns(self.frames)?;
        Ok(Some(bytes))
    }
}

fn pcm16_bytes(samples: &[f32], channels: u16) -> Result<Vec<u8>, String> {
    if !(1..=2).contains(&channels)
        || samples.is_empty()
        || !samples.len().is_multiple_of(usize::from(channels))
    {
        return Err("Ogg/Opus PCM channel alignment is invalid".into());
    }
    let byte_count = samples
        .len()
        .checked_mul(2)
        .filter(|bytes| *bytes <= MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES)
        .ok_or("Ogg/Opus PCM sample exceeds worker limit")?;
    let mut bytes = Vec::with_capacity(byte_count);
    for sample in samples {
        if !sample.is_finite() {
            return Err("Ogg/Opus produced a non-finite PCM sample".into());
        }
        // Header gain is applied by libopus before this output conversion.
        // Preserve unclipped float PCM in Web Audio, saturate only PCM16 playback.
        let sample = (sample.clamp(-1.0, 1.0) * 32_768.0)
            .round()
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(bytes)
}

pub(super) fn frames_to_100ns(frames: u64) -> Result<u64, String> {
    frames
        .checked_mul(10_000_000)
        .map(|value| value / u64::from(SAMPLE_RATE))
        .filter(|duration| *duration <= MAX_MEDIA_DURATION_100NS)
        .ok_or_else(|| "Ogg/Opus duration exceeds worker limit".into())
}
