use super::{metadata, packets};
use crate::encoded_audio::Budget;
use crate::opus_audio::Limits;
use std::collections::VecDeque;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::Instant;

pub(crate) struct Stream {
    decoder: opus::Decoder,
    packets: VecDeque<packets::Packet>,
    channels: u16,
    frames: u64,
    presented: u64,
    scratch: Vec<f32>,
    failed: bool,
}

impl Stream {
    pub(crate) fn open(
        source: Arc<[u8]>,
        limits: Limits,
        cancelled: Option<&AtomicBool>,
        deadline: Instant,
    ) -> Result<Self, String> {
        packets::check(cancelled, deadline)?;
        if source.is_empty() || source.len() > crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES {
            return Err("WebM Opus exceeds the 8 MiB complete-file limit".into());
        }
        let metadata = metadata::inspect(&source, &mut Budget::new(cancelled))?;
        let admitted = packets::admit(source, &metadata, limits, cancelled, deadline)?;
        packets::check(cancelled, deadline)?;
        let native = if metadata.head.channels == 1 {
            opus::Channels::Mono
        } else {
            opus::Channels::Stereo
        };
        let mut decoder = opus::Decoder::new(48_000, native)
            .map_err(|error| format!("create WebM Opus decoder: {error}"))?;
        decoder
            .set_gain(i32::from(metadata.head.gain))
            .map_err(|error| format!("apply WebM Opus gain: {error}"))?;
        Ok(Self {
            decoder,
            channels: metadata.head.channels,
            frames: admitted.frames,
            packets: admitted.packets.into(),
            presented: 0,
            scratch: vec![0.0; 5_760 * usize::from(metadata.head.channels)],
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
            return Err("WebM Opus stream failed; reopen to restart".into());
        }
        let result = self.decode_next(cancelled, deadline);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn decode_next(
        &mut self,
        cancelled: Option<&AtomicBool>,
        deadline: Instant,
    ) -> Result<Option<Vec<f32>>, String> {
        loop {
            packets::check(cancelled, deadline)?;
            let Some(packet) = self.packets.front_mut() else {
                if self.presented != self.frames {
                    return Err("WebM Opus presentation is incomplete".into());
                }
                return Ok(None);
            };
            if packet.gap != 0 {
                let frames = packet.gap.min(5_760);
                packet.gap -= frames;
                self.presented += frames;
                return Ok(Some(vec![
                    0.0;
                    frames as usize * usize::from(self.channels)
                ]));
            }
            let packet = self.packets.pop_front().unwrap();
            let count = self
                .decoder
                .decode_float(&packet.data, &mut self.scratch, false)
                .map_err(|error| format!("decode WebM Opus packet: {error}"))?;
            packets::check(cancelled, deadline)?;
            if count != packet.frames || count > 5_760 {
                return Err("WebM Opus decoder duration disagrees with admission".into());
            }
            if packet.right == packet.left {
                continue;
            }
            let channels = usize::from(self.channels);
            let output = &self.scratch[packet.left * channels..packet.right * channels];
            if output.iter().any(|sample| !sample.is_finite()) {
                return Err("WebM Opus produced non-finite PCM".into());
            }
            self.presented = self
                .presented
                .checked_add((packet.right - packet.left) as u64)
                .filter(|n| *n <= self.frames)
                .ok_or("WebM Opus exceeded its presentation frames")?;
            return Ok(Some(output.to_vec()));
        }
    }
}
