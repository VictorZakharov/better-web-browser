//! Elementary FLAC frames using the existing flacenc backend, not FLAC files.
//! The float bridge is quantized once to signed 24-bit PCM. Packetization never
//! pads a partial final block or includes metadata in EncodedAudioChunk data.
mod options;
#[cfg(test)]
mod tests;

use super::{Output, config::Config};
use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, Frame, FrameOffset, Stream, StreamInfo};
use flacenc::error::Verified;
use flacenc::source::{Fill, FrameBuf};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) use options::validate;

struct Span {
    timestamp: i64,
    frames: usize,
    consumed: usize,
}

pub(super) struct Encoder {
    config: Verified<flacenc::config::Encoder>,
    info: StreamInfo,
    description: Option<Vec<u8>>,
    rate: u32,
    channels: usize,
    block_size: usize,
    offset: u64,
    pending: Vec<i32>,
    spans: VecDeque<Span>,
}

impl Encoder {
    pub(super) fn new(config: &Config) -> Result<Self, String> {
        validate(config)?;
        let options = config.flac.clone().unwrap_or_default();
        let block_size = if options.block_size == 0 {
            4096
        } else {
            options.block_size as usize
        };
        let rate = config.sample_rate;
        let channels = config.number_of_channels as usize;
        let mut info = StreamInfo::new(rate as usize, channels, 24)
            .map_err(|error| format!("create FLAC stream metadata: {error}"))?;
        info.set_block_sizes(16, block_size)
            .map_err(|error| format!("set FLAC stream block sizes: {error}"))?;
        let mut sink = ByteSink::new();
        Stream::with_stream_info(info.clone())
            .write(&mut sink)
            .map_err(|error| format!("write FLAC stream metadata: {error}"))?;
        Ok(Self {
            config: options::backend(block_size, options.compress_level)?,
            info,
            description: Some(sink.into_inner()),
            rate,
            channels,
            block_size,
            offset: 0,
            pending: Vec::new(),
            spans: VecDeque::new(),
        })
    }

    pub(super) fn encode(
        &mut self,
        bytes: &[u8],
        timestamp: i64,
        cancelled: &AtomicBool,
    ) -> Result<Vec<Output>, String> {
        if cancelled.load(Ordering::Acquire) {
            return Err("FLAC encoding was cancelled".into());
        }
        if bytes.is_empty()
            || bytes.len() > super::MAX_INPUT_BYTES
            || !bytes.len().is_multiple_of(self.channels * 4)
        {
            return Err("FLAC input requires bounded complete float PCM frames".into());
        }
        // Validate the entire command before appending any samples or timestamps.
        let samples = bytes
            .chunks_exact(4)
            .map(|sample| f32::from_le_bytes(sample.try_into().unwrap()));
        if samples.clone().any(|sample| !sample.is_finite()) {
            return Err("FLAC encoder input contains non-finite PCM".into());
        }
        self.pending.extend(samples.map(|sample| {
            (f64::from(sample) * 8_388_608.0)
                .round()
                .clamp(-8_388_608.0, 8_388_607.0) as i32
        }));
        self.spans.push_back(Span {
            timestamp,
            frames: bytes.len() / (self.channels * 4),
            consumed: 0,
        });
        let mut outputs = Vec::new();
        while self.pending.len() / self.channels >= self.block_size {
            outputs.push(self.packet(self.block_size, cancelled)?);
        }
        Ok(outputs)
    }

    fn packet(&mut self, frames: usize, cancelled: &AtomicBool) -> Result<Output, String> {
        if cancelled.load(Ordering::Acquire) {
            return Err("FLAC encoding was cancelled".into());
        }
        let next_offset = self
            .offset
            .checked_add(frames as u64)
            .filter(|value| *value < 1 << 36)
            .ok_or("FLAC sample number exceeds its 36-bit limit")?;
        let mut buffer = FrameBuf::with_size(self.channels, frames.max(32))
            .map_err(|error| format!("allocate FLAC frame: {error}"))?;
        buffer
            .fill_interleaved(&self.pending[..frames * self.channels])
            .map_err(|error| format!("fill FLAC frame: {error}"))?;
        let frame = flacenc::encode_fixed_size_frame(&self.config, &buffer, 0, &self.info)
            .map_err(|error| format!("encode FLAC frame: {error}"))?;
        let (mut header, subframes) = frame.into_parts();
        header.set_frame_offset(FrameOffset::StartSample(self.offset));
        let frame = Frame::new(header, subframes.into_iter())
            .map_err(|error| format!("construct FLAC frame: {error}"))?;
        let mut sink = ByteSink::new();
        frame
            .write(&mut sink)
            .map_err(|error| format!("write FLAC frame: {error}"))?;
        let span = self
            .spans
            .front()
            .ok_or("FLAC frame has no presentation timestamp")?;
        let timestamp = span
            .timestamp
            .checked_add((span.consumed as u64 * 1_000_000 / u64::from(self.rate)) as i64)
            .ok_or("FLAC presentation timestamp overflows")?;
        let mut remaining = frames;
        while remaining > 0 {
            let span = self
                .spans
                .front_mut()
                .ok_or("FLAC frame exceeds retained PCM timing")?;
            let take = remaining.min(span.frames - span.consumed);
            span.consumed += take;
            remaining -= take;
            if span.consumed == span.frames {
                self.spans.pop_front();
            }
        }
        self.pending.drain(..frames * self.channels);
        self.offset = next_offset;
        Ok(Output {
            bytes: sink.into_inner(),
            format: "flac",
            timestamp,
            duration: frames as u64 * 1_000_000 / u64::from(self.rate),
            frames: 0,
            sample_rate: self.rate,
            channels: self.channels as u32,
            description: self.description.take(),
        })
    }

    pub(super) fn flush(&mut self, cancelled: &AtomicBool) -> Result<Vec<Output>, String> {
        let frames = self.pending.len() / self.channels;
        if frames == 0 {
            return Ok(Vec::new());
        }
        Ok(vec![self.packet(frames, cancelled)?])
    }
}
