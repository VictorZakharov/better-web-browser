//! Incremental lossless encoding; preserve the existing FLAC recording policy.

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, Frame, FrameOffset, Stream, StreamInfo};
use flacenc::config;
use flacenc::error::{Verified, Verify};
use flacenc::source::{Fill, FrameBuf};

const MAX_INPUT_FRAMES: usize = 960;
const MIN_FRAME_SAMPLES: usize = 16;
const MIN_BUFFER_CAPACITY: usize = 32;
const MAX_SAMPLE_OFFSET: u64 = (1 << 36) - 1;

pub(super) struct Session {
    config: Verified<config::Encoder>,
    info: Option<StreamInfo>,
    format: Option<(usize, usize)>,
    pending: Vec<u8>,
    sample_offset: u64,
}

impl Session {
    pub(super) fn new() -> Result<Self, &'static str> {
        let mut config = config::Encoder::default();
        config.block_size = MAX_INPUT_FRAMES;
        config.subframe_coding.use_lpc = false;
        Ok(Self {
            config: config
                .into_verified()
                .map_err(|_| "Invalid FLAC encoder configuration")?,
            info: None,
            format: None,
            pending: Vec::with_capacity(MAX_INPUT_FRAMES * 4),
            sample_offset: 0,
        })
    }

    pub(super) fn format_supported(&self, rate: usize, channels: usize) -> bool {
        (8_000..=48_000).contains(&rate)
            && (1..=2).contains(&channels)
            && self.format.is_none_or(|format| format == (rate, channels))
    }

    pub(super) fn append(
        &mut self,
        sample_rate: usize,
        channels: usize,
        pcm: &[u8],
    ) -> Result<Vec<u8>, &'static str> {
        if !self.format_supported(sample_rate, channels)
            || pcm.is_empty()
            || pcm.len() > MAX_INPUT_FRAMES * channels * 2
            || !pcm.len().is_multiple_of(channels * 2)
        {
            return Err("Invalid or changed capture PCM format");
        }
        let mut output = Vec::new();
        if self.info.is_none() {
            let mut info = StreamInfo::new(sample_rate, channels, 16)
                .map_err(|_| "Invalid FLAC stream format")?;
            // STREAMINFO sample total/MD5 remain unknown. Variable final blocks
            // preserve short tails (RFC 9639 8.2), without buffering the stream.
            info.set_block_sizes(MIN_FRAME_SAMPLES, MAX_INPUT_FRAMES)
                .map_err(|_| "Invalid FLAC block sizes")?;
            info.set_frame_sizes(0, 0)
                .map_err(|_| "Invalid FLAC frame sizes")?;
            let mut sink = ByteSink::new();
            Stream::with_stream_info(info.clone())
                .write(&mut sink)
                .map_err(|_| "Could not write FLAC header")?;
            output.extend_from_slice(sink.as_slice());
            self.info = Some(info);
            self.format = Some((sample_rate, channels));
        }
        self.pending.extend_from_slice(pcm);
        let stride = channels * 2;
        while self.pending.len() / stride >= MIN_FRAME_SAMPLES {
            let frames = (self.pending.len() / stride).min(MAX_INPUT_FRAMES);
            output.extend(self.encode_frame(frames)?);
        }
        Ok(output)
    }

    pub(super) fn finish(&mut self) -> Result<Vec<u8>, &'static str> {
        let Some((_, channels)) = self.format else {
            return Ok(Vec::new());
        };
        let frames = self.pending.len() / (channels * 2);
        if frames == 0 {
            return Ok(Vec::new());
        }
        self.encode_frame(frames)
    }

    fn encode_frame(&mut self, frames: usize) -> Result<Vec<u8>, &'static str> {
        let info = self.info.as_ref().ok_or("Missing FLAC stream header")?;
        let channels = info.channels();
        let byte_count = frames * channels * 2;
        let next_offset = self
            .sample_offset
            .checked_add(frames as u64)
            .filter(|offset| *offset <= MAX_SAMPLE_OFFSET)
            .ok_or("FLAC recording duration limit reached")?;
        let mut buffer = FrameBuf::with_size(channels, frames.max(MIN_BUFFER_CAPACITY))
            .map_err(|_| "Invalid FLAC frame size")?;
        buffer
            .fill_le_bytes(&self.pending[..byte_count], 2)
            .map_err(|_| "Invalid capture PCM samples")?;
        let frame = flacenc::encode_fixed_size_frame(&self.config, &buffer, 0, info)
            .map_err(|_| "FLAC frame encoding failed")?;
        let (mut header, subframes) = frame.into_parts();
        header.set_frame_offset(FrameOffset::StartSample(self.sample_offset));
        let frame = Frame::new(header, subframes.into_iter())
            .map_err(|_| "FLAC frame construction failed")?;
        let mut sink = ByteSink::new();
        frame
            .write(&mut sink)
            .map_err(|_| "Could not write FLAC frame")?;
        self.pending.drain(..byte_count);
        self.sample_offset = next_offset;
        Ok(sink.into_inner())
    }
}
