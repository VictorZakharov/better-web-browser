use super::stream::PcmStream;
use crate::limits::{
    MAX_MEDIA_DECODED_SAMPLES, MAX_MEDIA_ENCODED_QUEUE_BYTES, MEDIA_COMMAND_TIMEOUT,
};
use crate::opus_audio::SAMPLE_RATE;
use std::sync::Arc;
use std::time::Instant;

pub(in crate::media_process) struct OpusDecoder {
    source: Arc<[u8]>,
    stream: PcmStream,
    channels: u16,
    expected_samples: u32,
    pending: Option<Vec<u8>>,
}

impl OpusDecoder {
    pub(in crate::media_process) fn open(
        bytes: &[u8],
        expected_samples: u32,
        expected_sample_rate: u32,
        expected_channels: u16,
    ) -> Result<Self, String> {
        if bytes.is_empty()
            || bytes.len() > MAX_MEDIA_ENCODED_QUEUE_BYTES
            || expected_samples == 0
            || expected_samples as usize > MAX_MEDIA_DECODED_SAMPLES
        {
            return Err("Ogg/Opus playback exceeds worker limits".into());
        }
        let source: Arc<[u8]> = Arc::from(bytes);
        let stream = PcmStream::open(source.clone(), Instant::now() + MEDIA_COMMAND_TIMEOUT)?;
        if expected_sample_rate != SAMPLE_RATE || expected_channels != stream.channels {
            return Err("Ogg/Opus format disagreed with decode report".into());
        }
        Ok(Self {
            source,
            stream,
            channels: expected_channels,
            expected_samples,
            pending: None,
        })
    }

    pub(in crate::media_process) fn seek(&mut self, position_100ns: u64) -> Result<(), String> {
        let target_frame =
            u64::try_from(u128::from(position_100ns) * u128::from(SAMPLE_RATE) / 10_000_000)
                .map_err(|_| "Ogg/Opus seek target overflow")?;
        // Restart applies gain, pre-skip and EOS trimming exactly once, even
        // when paused clients issue several seeks without requesting PCM.
        let deadline = Instant::now() + MEDIA_COMMAND_TIMEOUT;
        self.stream = PcmStream::open(self.source.clone(), deadline)?;
        self.pending = None;
        let mut frame = 0_u64;
        while frame < target_frame {
            let Some(bytes) = self.read_packet(deadline)? else {
                break;
            };
            let end = frame
                .checked_add(bytes.len() as u64 / (u64::from(self.channels) * 2))
                .ok_or("Ogg/Opus seek frame count overflow")?;
            if end > target_frame {
                let skip = usize::try_from(target_frame - frame)
                    .ok()
                    .and_then(|frames| frames.checked_mul(usize::from(self.channels) * 2))
                    .ok_or("Ogg/Opus seek offset overflow")?;
                self.pending = Some(bytes[skip..].to_vec());
                break;
            }
            frame = end;
        }
        Ok(())
    }

    pub(in crate::media_process) fn next_sample(&mut self) -> Result<Option<Vec<u8>>, String> {
        if let Some(bytes) = self.pending.take() {
            return Ok(Some(bytes));
        }
        self.read_packet(Instant::now() + MEDIA_COMMAND_TIMEOUT)
    }

    fn read_packet(&mut self, deadline: Instant) -> Result<Option<Vec<u8>>, String> {
        let pcm = self.stream.next_sample(deadline)?;
        if self.stream.samples > self.expected_samples {
            return Err("Ogg/Opus playback exceeded decoded report".into());
        }
        if pcm.is_none() && self.stream.samples != self.expected_samples {
            return Err("Ogg/Opus playback ended before decoded report".into());
        }
        Ok(pcm)
    }
}
