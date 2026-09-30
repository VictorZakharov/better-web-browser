//! Bounded incremental RFC 7845 family-0 recording, using the vetted encoder.
//! No resampling: unsupported capture rates fail rather than relabeling PCM.

use ogg::writing::{PacketWriteEndInfo, PacketWriter};

const MAX_INPUT_FRAMES: usize = 960;
const MAX_PACKET_BYTES: usize = 4_000;
const MAX_PACKETS: usize = 16_384;
const MAX_DURATION_FRAMES: u64 = 48_000 * 3_600;

pub(super) struct Session {
    serial: u32,
    bitrate: u32,
    constant: bool,
    encoder: Option<Encoding>,
    failed: bool,
    blocked: bool,
    closed: bool,
}

struct Encoding {
    codec: opus::Encoder,
    writer: PacketWriter<Vec<u8>>,
    rate: usize,
    channels: usize,
    pending: Vec<i16>,
    pre_skip: u16,
    input_frames: u64,
    encoded_frames: u64,
    encoded_bytes: usize,
    packets: usize,
    scratch: Vec<u8>,
}

impl Session {
    pub(super) fn new(serial: u32, bitrate: u32, constant: bool) -> Self {
        Self {
            serial,
            bitrate,
            constant,
            encoder: None,
            failed: false,
            blocked: false,
            closed: false,
        }
    }

    pub(super) fn format_supported(&self, rate: usize, channels: usize) -> bool {
        matches!(rate, 8_000 | 12_000 | 16_000 | 24_000 | 48_000)
            && (1..=2).contains(&channels)
            && self
                .encoder
                .as_ref()
                .is_none_or(|encoder| encoder.rate == rate && encoder.channels == channels)
    }

    pub(super) fn append(
        &mut self,
        rate: usize,
        channels: usize,
        pcm: &[u8],
    ) -> Result<Vec<u8>, &'static str> {
        if self.failed || self.blocked || self.closed {
            return Err("Ogg Opus recorder failed; start a new recording");
        }
        if !self.format_supported(rate, channels) {
            self.failed = true;
            return Err("Ogg Opus requires unchanged native 8/12/16/24/48 kHz mono/stereo capture");
        }
        if pcm.is_empty()
            || pcm.len() > MAX_INPUT_FRAMES * channels * 2
            || !pcm.len().is_multiple_of(channels * 2)
        {
            self.failed = true;
            return Err("Invalid Ogg Opus capture PCM packet");
        }
        if self.encoder.is_none() {
            self.encoder = Some(Encoding::new(
                self.serial,
                rate,
                channels,
                self.bitrate,
                self.constant,
            )?);
        }
        let encoder = self.encoder.as_mut().unwrap();
        let frames = (pcm.len() / (channels * 2)) as u64;
        if let Err(error) = encoder.reserve_append(frames) {
            // No new PCM entered the predictive state. Seal the admitted prefix
            // on stop/error rather than emitting an unplayable truncated Ogg.
            self.blocked = true;
            return Err(error);
        }
        let result = encoder.append(self.serial, frames, pcm);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub(super) fn finish(&mut self) -> Result<Vec<u8>, &'static str> {
        if self.closed {
            return Err("Ogg Opus recorder is already closed");
        }
        self.closed = true;
        if self.failed {
            return Err("Ogg Opus recorder failed; discard the incomplete recording");
        }
        let Some(encoder) = self.encoder.as_mut() else {
            return Ok(Vec::new());
        };
        encoder.reserve_append(0)?;
        let final_granule = encoder
            .presentation_frames()?
            .checked_add(u64::from(encoder.pre_skip))
            .ok_or("Ogg Opus final granule overflows")?;
        let packet_samples = (encoder.rate / 50) * encoder.channels;
        // Supply real encoder history plus zero padding until lookahead drains.
        // EOS trims exactly that padding; never invent PLC or a second stream.
        loop {
            encoder.pending.resize(packet_samples, 0);
            let next = encoder
                .encoded_frames
                .checked_add(960)
                .ok_or("Ogg Opus raw duration overflows")?;
            let ending = if next >= final_granule {
                PacketWriteEndInfo::EndStream
            } else {
                PacketWriteEndInfo::EndPage
            };
            encoder.encode(self.serial, ending, Some(final_granule.min(next)))?;
            if next >= final_granule {
                break;
            }
        }
        encoder.drain()
    }
}

impl Encoding {
    fn reserve_append(&self, frames: u64) -> Result<(), &'static str> {
        let raw = self
            .input_frames
            .checked_add(frames)
            .and_then(|frames| frames.checked_mul((48_000 / self.rate) as u64))
            .and_then(|frames| frames.checked_add(u64::from(self.pre_skip)))
            .ok_or("Ogg Opus recording duration overflows")?;
        // Even a hypothetical zero-lookahead encoder needs a packet carrying
        // EOS; reserve at least one final packet beyond those already emitted.
        let packets = raw.div_ceil(960).max(self.packets as u64 + 1);
        if packets > MAX_PACKETS as u64 || packets * 960 > MAX_DURATION_FRAMES {
            return Err("Ogg Opus recording exceeds its packet or raw duration limit");
        }
        let remaining = usize::try_from(packets)
            .ok()
            .and_then(|packets| packets.checked_sub(self.packets))
            .ok_or("Ogg Opus recording packet accounting is inconsistent")?;
        // Conservative packet-sized reservation includes EOS flush and lacing.
        // Draining Blob chunks never resets this cumulative complete-file bound.
        let page_bytes = MAX_PACKET_BYTES + 27 + MAX_PACKET_BYTES / 255 + 1;
        let reserved = remaining
            .checked_mul(page_bytes)
            .and_then(|bytes| bytes.checked_add(self.writer.inner().len()))
            .and_then(|bytes| bytes.checked_add(self.encoded_bytes))
            .ok_or("Ogg Opus recording byte count overflows")?;
        if reserved > crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES {
            return Err("Ogg Opus recording exceeds the 8 MiB complete-file limit");
        }
        Ok(())
    }

    fn append(&mut self, serial: u32, frames: u64, pcm: &[u8]) -> Result<Vec<u8>, &'static str> {
        self.input_frames += frames; // reserve_append checked this exact sum.
        self.pending.extend(
            pcm.chunks_exact(2)
                .map(|sample| i16::from_le_bytes([sample[0], sample[1]])),
        );
        let packet_samples = (self.rate / 50) * self.channels;
        while self.pending.len() >= packet_samples {
            self.encode(serial, PacketWriteEndInfo::EndPage, None)?;
        }
        self.drain()
    }

    fn new(
        serial: u32,
        rate: usize,
        channels: usize,
        bitrate: u32,
        constant: bool,
    ) -> Result<Self, &'static str> {
        let native_channels = if channels == 1 {
            opus::Channels::Mono
        } else {
            opus::Channels::Stereo
        };
        let mut codec = opus::Encoder::new(rate as u32, native_channels, opus::Application::Audio)
            .map_err(|_| "Could not create Ogg Opus encoder")?;
        codec
            .set_complexity(5)
            .map_err(|_| "Could not bound Ogg Opus encoder complexity")?;
        codec
            .set_bitrate(opus::Bitrate::Bits(bitrate as i32))
            .map_err(|_| "Could not configure Ogg Opus target bitrate")?;
        codec
            .set_vbr(!constant)
            .map_err(|_| "Could not configure Ogg Opus bitrate mode")?;
        codec
            .set_dtx(false)
            .map_err(|_| "Could not configure Ogg Opus continuous recording")?;
        let lookahead = codec
            .get_lookahead()
            .map_err(|_| "Could not query Ogg Opus encoder delay")?;
        let pre_skip = u16::try_from(i64::from(lookahead) * (48_000 / rate) as i64)
            .map_err(|_| "Ogg Opus encoder delay exceeds the mapping header")?;
        let mut head = b"OpusHead".to_vec();
        head.extend([1, channels as u8]);
        head.extend_from_slice(&pre_skip.to_le_bytes());
        head.extend_from_slice(&(rate as u32).to_le_bytes());
        head.extend_from_slice(&0_i16.to_le_bytes()); // No additional recording gain.
        head.push(0);
        let mut tags = b"OpusTags".to_vec();
        tags.extend_from_slice(&6_u32.to_le_bytes());
        tags.extend_from_slice(b"Breeze");
        tags.extend_from_slice(&0_u32.to_le_bytes());
        let mut writer = PacketWriter::new(Vec::new());
        for header in [head, tags] {
            writer
                .write_packet(
                    header.into_boxed_slice(),
                    serial,
                    PacketWriteEndInfo::EndPage,
                    0,
                )
                .map_err(|_| "Could not write Ogg Opus mapping headers")?;
        }
        Ok(Self {
            codec,
            writer,
            rate,
            channels,
            pending: Vec::with_capacity(MAX_INPUT_FRAMES * channels),
            pre_skip,
            input_frames: 0,
            encoded_frames: 0,
            encoded_bytes: 0,
            packets: 0,
            scratch: vec![0; MAX_PACKET_BYTES],
        })
    }

    fn presentation_frames(&self) -> Result<u64, &'static str> {
        self.input_frames
            .checked_mul((48_000 / self.rate) as u64)
            .ok_or("Ogg Opus presentation duration overflows")
    }

    fn encode(
        &mut self,
        serial: u32,
        ending: PacketWriteEndInfo,
        granule: Option<u64>,
    ) -> Result<(), &'static str> {
        if self.packets >= MAX_PACKETS {
            return Err("Ogg Opus recording exceeds the 16,384-packet limit");
        }
        let packet_samples = (self.rate / 50) * self.channels;
        let next = self
            .encoded_frames
            .checked_add(960)
            .filter(|raw| *raw <= MAX_DURATION_FRAMES)
            .ok_or("Ogg Opus recording exceeds the raw duration limit")?;
        let bytes = self
            .codec
            .encode(&self.pending[..packet_samples], &mut self.scratch)
            .map_err(|_| "Ogg Opus PCM encoding failed")?;
        if bytes == 0 || bytes > self.scratch.len() {
            return Err("Ogg Opus encoder returned an invalid packet");
        }
        self.writer
            .write_packet(
                self.scratch[..bytes].to_vec().into_boxed_slice(),
                serial,
                ending,
                granule.unwrap_or(next),
            )
            .map_err(|_| "Could not write Ogg Opus audio packet")?;
        self.pending.drain(..packet_samples);
        self.encoded_frames = next;
        self.packets += 1;
        Ok(())
    }

    fn drain(&mut self) -> Result<Vec<u8>, &'static str> {
        let bytes = std::mem::take(self.writer.inner_mut());
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(bytes.len())
            .filter(|total| *total <= crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES)
            .ok_or("Ogg Opus recording exceeds the 8 MiB complete-file limit")?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests;
