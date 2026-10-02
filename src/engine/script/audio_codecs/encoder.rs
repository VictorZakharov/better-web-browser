//! Bounded packetization over the existing libopus wrapper. No codec bitstream
//! implementation is copied here; the only written header is RFC 7845 metadata.
use super::{Output, config::Config};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

struct Span {
    timestamp: i64,
    frames: usize,
    consumed: usize,
}

pub(super) struct Encoder {
    codec: opus::Encoder,
    rate: u32,
    channels: usize,
    packet_frames: usize,
    frame_duration: u32,
    lookahead: usize,
    pending: Vec<f32>,
    spans: VecDeque<Span>,
    description: Option<Vec<u8>>,
    metadata_pending: bool,
    tail_frames: usize,
    next_timestamp: Option<i64>,
}

impl Encoder {
    pub(super) fn new(config: &Config) -> Result<Self, String> {
        let options = config.opus.clone().unwrap_or_default();
        let application = match options.application.as_str() {
            "voip" => opus::Application::Voip,
            "lowdelay" => opus::Application::LowDelay,
            _ => opus::Application::Audio,
        };
        let mut codec = opus::Encoder::new(config.sample_rate, config.channels(), application)
            .map_err(|e| format!("create Opus packet encoder: {e}"))?;
        let bitrate = config.bitrate.map_or(opus::Bitrate::Auto, |value| {
            opus::Bitrate::Bits(value as i32)
        });
        codec.set_bitrate(bitrate).map_err(codec_error)?;
        codec
            .set_vbr(config.bitrate_mode.as_deref() != Some("constant"))
            .map_err(codec_error)?;
        codec
            .set_complexity(options.complexity as i32)
            .map_err(codec_error)?;
        codec
            .set_packet_loss_perc(options.packetlossperc as i32)
            .map_err(codec_error)?;
        codec
            .set_inband_fec(options.useinbandfec)
            .map_err(codec_error)?;
        codec.set_dtx(options.usedtx).map_err(codec_error)?;
        codec
            .set_signal(match options.signal.as_str() {
                "music" => opus::Signal::Music,
                "voice" => opus::Signal::Voice,
                _ => opus::Signal::Auto,
            })
            .map_err(codec_error)?;
        let lookahead = usize::try_from(codec.get_lookahead().map_err(codec_error)?)
            .map_err(|_| "negative Opus encoder lookahead")?;
        let description = if options.format == "ogg" {
            let pre_skip = u16::try_from(lookahead * 48_000 / config.sample_rate as usize)
                .map_err(|_| "Opus encoder lookahead exceeds identification header")?;
            let mut header = b"OpusHead".to_vec();
            header.extend([1, config.number_of_channels as u8]);
            header.extend(pre_skip.to_le_bytes());
            header.extend(config.sample_rate.to_le_bytes());
            header.extend(0_i16.to_le_bytes());
            header.push(0);
            Some(header)
        } else {
            None
        };
        Ok(Self {
            codec,
            rate: config.sample_rate,
            channels: config.number_of_channels as usize,
            packet_frames: (u64::from(config.sample_rate) * u64::from(options.frame_duration)
                / 1_000_000) as usize,
            frame_duration: options.frame_duration,
            lookahead,
            pending: Vec::new(),
            spans: VecDeque::new(),
            description,
            metadata_pending: true,
            tail_frames: 0,
            next_timestamp: None,
        })
    }

    pub(super) fn encode(
        &mut self,
        bytes: &[u8],
        timestamp: i64,
        cancelled: &AtomicBool,
    ) -> Result<Vec<Output>, String> {
        let stride = self.channels * 4;
        if bytes.is_empty()
            || bytes.len() > super::MAX_INPUT_BYTES
            || !bytes.len().is_multiple_of(stride)
        {
            return Err("Opus input must contain bounded complete interleaved float frames".into());
        }
        let frames = bytes.len() / stride;
        if self.pending.len() + bytes.len() / 4
            > super::MAX_INPUT_BYTES / 4 + self.packet_frames * self.channels
        {
            return Err("Opus pending samples exceed the packetization budget".into());
        }
        let samples = bytes
            .chunks_exact(4)
            .map(|sample| f32::from_le_bytes(sample.try_into().unwrap()))
            .collect::<Vec<_>>();
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err("Opus input contains non-finite PCM".into());
        }
        // Input timestamps need not be continuous. Span ownership retains the
        // timestamp of the first real sample in each assembled packet.
        self.spans.push_back(Span {
            timestamp,
            frames,
            consumed: 0,
        });
        self.pending.extend(samples);
        self.tail_frames = self.lookahead;
        let mut outputs = Vec::new();
        while self.pending.len() >= self.packet_frames * self.channels {
            outputs.push(self.packet(cancelled)?);
        }
        Ok(outputs)
    }

    fn packet(&mut self, cancelled: &AtomicBool) -> Result<Output, String> {
        if cancelled.load(Ordering::Acquire) {
            return Err("Opus encoding cancelled".into());
        }
        let timestamp = if let Some(span) = self.spans.front() {
            span.timestamp
                .checked_add((span.consumed as u64 * 1_000_000 / u64::from(self.rate)) as i64)
                .and_then(|time| {
                    time.checked_sub(
                        (self.lookahead as u64 * 1_000_000 / u64::from(self.rate)) as i64,
                    )
                })
                .ok_or("Opus packet timestamp overflows")?
        } else {
            self.next_timestamp
                .ok_or("Opus has no pending presentation timestamp")?
        };
        let samples = self.packet_frames * self.channels;
        let bytes = super::packet_encoder::encode(
            &mut self.codec,
            &self.pending[..samples],
            self.rate,
            self.channels,
            self.frame_duration,
            cancelled,
        )?;
        self.pending.drain(..samples);
        let mut consumed = self.packet_frames;
        while consumed > 0 {
            let Some(span) = self.spans.front_mut() else {
                self.tail_frames = self.tail_frames.saturating_sub(consumed);
                break;
            };
            let take = consumed.min(span.frames - span.consumed);
            consumed -= take;
            span.consumed += take;
            if span.consumed == span.frames {
                self.spans.pop_front();
            }
        }
        let duration = self.packet_frames as u64 * 1_000_000 / u64::from(self.rate);
        self.next_timestamp = timestamp.checked_add(duration as i64);
        if self.next_timestamp.is_none() {
            return Err("Opus next timestamp overflows".into());
        }
        let description = if self.metadata_pending {
            self.metadata_pending = false;
            self.description.clone()
        } else {
            None
        };
        Ok(Output {
            bytes,
            format: "opus",
            timestamp,
            duration,
            frames: 0,
            description,
        })
    }

    pub(super) fn flush(&mut self, cancelled: &AtomicBool) -> Result<Vec<Output>, String> {
        let mut outputs = Vec::new();
        // Real pending samples plus encoder history must emerge before flush
        // resolves. Raw Opus packets have no EOS trim field: final padding is
        // deliberately observable, rather than mislabeled as lossless duration.
        while !self.pending.is_empty() || self.tail_frames != 0 {
            self.pending.resize(self.packet_frames * self.channels, 0.0);
            outputs.push(self.packet(cancelled)?);
        }
        Ok(outputs)
    }
}

fn codec_error(error: opus::Error) -> String {
    format!("Opus packet encoder: {error}")
}
