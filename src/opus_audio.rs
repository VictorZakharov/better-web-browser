//! Complete-file, mono/stereo Opus decoding in Ogg and WebM containers.
//! The upstream readers and libopus do the demuxing and codec work. Admission
//! validates the complete immutable source before any predictive PCM decoding.

use ogg::reading::PacketReader;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

mod admission;
mod container;
pub(crate) use container::Stream;
#[cfg(test)]
mod tests;
mod timeline;

pub(crate) const SAMPLE_RATE: u32 = 48_000;
const MAX_PACKET_BYTES: usize = 61_440;
const MAX_PACKET_FRAMES: usize = 5_760;
const MAX_PACKETS: usize = 16_384;
const MAX_DURATION_FRAMES: u64 = SAMPLE_RATE as u64 * 3_600;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub(crate) max_decoded_bytes: usize,
    pub(crate) max_packets: usize,
    pub(crate) max_duration_frames: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_decoded_bytes: crate::limits::MAX_MEDIA_DECODED_SOURCE_BYTES as usize,
            max_packets: MAX_PACKETS,
            max_duration_frames: MAX_DURATION_FRAMES,
        }
    }
}

/// Recognize the first physical packet, including unsupported/malformed heads;
/// never search arbitrary comments or audio bytes for a codec signature.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    crate::webm_opus::sniff(bytes)
        || (bytes.starts_with(b"OggS")
            && bytes.get(26).is_some_and(|segments| {
                let offset = 27 + usize::from(*segments);
                bytes.get(offset..offset + 8) == Some(b"OpusHead")
            }))
}

pub(super) struct OggStream {
    reader: PacketReader<Cursor<Arc<[u8]>>>,
    decoder: opus::Decoder,
    channels: u16,
    frames: u64,
    start: u64,
    end: u64,
    raw_frames: u64,
    raw_position: u64,
    presented: u64,
    durations: Vec<u16>,
    packet_index: usize,
    scratch: Vec<f32>,
    finished: bool,
    failed: bool,
}

impl OggStream {
    pub(crate) fn open(
        source: Arc<[u8]>,
        limits: Limits,
        cancelled: Option<&AtomicBool>,
        deadline: Instant,
    ) -> Result<Self, String> {
        let mut budget = Budget::new(cancelled, deadline);
        budget.step()?;
        if source.is_empty() || source.len() > crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES {
            return Err("Ogg Opus exceeds the 8 MiB complete-file source limit".into());
        }
        let admitted = admission::validate(&source, limits, &mut budget)?;
        budget.step()?;
        let channels = match admitted.channels {
            1 => opus::Channels::Mono,
            2 => opus::Channels::Stereo,
            _ => return Err("Ogg Opus channel mapping is unsupported".into()),
        };
        let mut decoder = opus::Decoder::new(SAMPLE_RATE, channels)
            .map_err(|error| format!("create Ogg Opus decoder: {error}"))?;
        decoder
            .set_gain(i32::from(admitted.gain))
            .map_err(|error| format!("apply Ogg Opus output gain: {error}"))?;
        let mut reader = PacketReader::new(Cursor::new(source));
        for _ in 0..2 {
            reader
                .read_packet()
                .map_err(|error| format!("reopen Ogg Opus headers: {error}"))?
                .ok_or("validated Ogg Opus header disappeared")?;
        }
        Ok(Self {
            reader,
            decoder,
            channels: admitted.channels,
            frames: admitted.end - u64::from(admitted.pre_skip),
            start: u64::from(admitted.pre_skip),
            end: admitted.end,
            raw_frames: admitted.raw_frames,
            raw_position: 0,
            presented: 0,
            durations: admitted.durations,
            packet_index: 0,
            scratch: vec![0.0; MAX_PACKET_FRAMES * usize::from(admitted.channels)],
            finished: false,
            failed: false,
        })
    }

    pub(crate) fn channels(&self) -> u16 {
        self.channels
    }

    pub(crate) fn frames(&self) -> u64 {
        self.frames
    }

    pub(crate) fn next_pcm(
        &mut self,
        cancelled: Option<&AtomicBool>,
        deadline: Instant,
    ) -> Result<Option<Vec<f32>>, String> {
        if self.failed {
            return Err("Ogg Opus stream failed; reopen the source to restart".into());
        }
        let result = self.decode_next(&mut Budget::new(cancelled, deadline));
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn decode_next(&mut self, budget: &mut Budget<'_>) -> Result<Option<Vec<f32>>, String> {
        loop {
            budget.step()?;
            if self.finished {
                return Ok(None);
            }
            let Some(packet) = self
                .reader
                .read_packet()
                .map_err(|error| format!("read validated Ogg Opus audio: {error}"))?
            else {
                if self.packet_index != self.durations.len()
                    || self.raw_position != self.raw_frames
                    || self.presented != self.frames
                {
                    return Err("Ogg Opus decoded sample count is incomplete".into());
                }
                self.finished = true;
                return Ok(None);
            };
            let expected = usize::from(
                *self
                    .durations
                    .get(self.packet_index)
                    .ok_or("Ogg Opus contains unexpected audio packets")?,
            );
            // Admission forbids an empty packet: empty native input requests PLC.
            budget.step()?;
            let decoded = self
                .decoder
                .decode_float(&packet.data, &mut self.scratch, false)
                .map_err(|error| format!("decode Ogg Opus packet: {error}"))?;
            budget.step()?;
            if decoded != expected || decoded > MAX_PACKET_FRAMES {
                return Err("Ogg Opus decoder returned an inconsistent packet duration".into());
            }
            let packet_start = self.raw_position;
            self.raw_position = self
                .raw_position
                .checked_add(decoded as u64)
                .filter(|position| *position <= self.raw_frames)
                .ok_or("Ogg Opus decoded position exceeds admission")?;
            self.packet_index += 1;
            let left = packet_start.max(self.start);
            let right = self.raw_position.min(self.end);
            if left >= right {
                continue;
            }
            let channels = usize::from(self.channels);
            let first = usize::try_from(left - packet_start)
                .map_err(|_| "Ogg Opus trim position overflows")?
                * channels;
            let last = usize::try_from(right - packet_start)
                .map_err(|_| "Ogg Opus trim position overflows")?
                * channels;
            let output = &self.scratch[first..last];
            if output.iter().any(|sample| !sample.is_finite()) {
                return Err("Ogg Opus decoder produced non-finite PCM".into());
            }
            self.presented = self
                .presented
                .checked_add(right - left)
                .filter(|frames| *frames <= self.frames)
                .ok_or("Ogg Opus presentation exceeds admission")?;
            return Ok(Some(output.to_vec()));
        }
    }
}

struct Budget<'a> {
    cancelled: Option<&'a AtomicBool>,
    deadline: Instant,
    units: usize,
}

impl<'a> Budget<'a> {
    fn new(cancelled: Option<&'a AtomicBool>, deadline: Instant) -> Self {
        Self {
            cancelled,
            deadline,
            units: 0,
        }
    }

    fn step(&mut self) -> Result<(), String> {
        if self
            .cancelled
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err("Ogg Opus decoding was cancelled".into());
        }
        if Instant::now() >= self.deadline {
            return Err("Ogg Opus decoding exceeded its operation deadline".into());
        }
        self.units += 1;
        if self.units > 65_536 {
            return Err("Ogg Opus exceeds the bounded admission complexity limit".into());
        }
        Ok(())
    }
}
