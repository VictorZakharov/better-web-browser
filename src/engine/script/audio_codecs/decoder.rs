//! Raw Opus packets, not a hidden container decoder or packet-loss concealment.
use super::{Output, config::Config};

pub(super) struct Decoder {
    codec: opus::Decoder,
    rate: u32,
    channels: usize,
    skip: usize,
}

impl Decoder {
    pub(super) fn new(config: &Config) -> Result<Self, String> {
        let mut codec = opus::Decoder::new(config.sample_rate, config.channels())
            .map_err(|e| format!("create Opus packet decoder: {e}"))?;
        let mut skip = 0;
        if let Some(bytes) = &config.description {
            let header = crate::webm_opus::header::read(bytes)?;
            codec
                .set_gain(i32::from(header.gain))
                .map_err(|e| format!("set Opus identification gain: {e}"))?;
            // RFC 7845 pre-skip is expressed at 48 kHz, independently of the
            // configured native output rate. Discard only complete samples.
            skip = usize::from(header.pre_skip) * config.sample_rate as usize / 48_000;
        }
        Ok(Self {
            codec,
            rate: config.sample_rate,
            channels: config.number_of_channels as usize,
            skip,
        })
    }

    pub(super) fn decode(&mut self, bytes: &[u8], timestamp: i64) -> Result<Vec<Output>, String> {
        if bytes.is_empty() || bytes.len() > super::MAX_PACKET_BYTES {
            return Err("Opus packet is empty or exceeds its 60 KiB packet budget".into());
        }
        // The upstream packet parser validates the full framing before state is
        // mutated. Empty input would request PLC, never actual input decoding.
        opus::packet::parse(bytes).map_err(|e| format!("invalid Opus packet: {e}"))?;
        let frames = opus::packet::get_nb_samples(bytes, self.rate)
            .map_err(|e| format!("invalid Opus packet duration: {e}"))?;
        if frames == 0 || frames > self.rate as usize * 120 / 1000 {
            return Err("Opus packet exceeds the 120 ms frame limit".into());
        }
        let mut pcm = vec![0.0_f32; frames * self.channels];
        let decoded = self
            .codec
            .decode_float(bytes, &mut pcm, false)
            .map_err(|e| format!("decode Opus packet: {e}"))?;
        if decoded != frames || pcm.iter().any(|sample| !sample.is_finite()) {
            return Err("Opus decoder returned inconsistent or non-finite PCM".into());
        }
        let skip = self.skip.min(frames);
        self.skip -= skip;
        if skip == frames {
            return Ok(Vec::new());
        }
        let timestamp = timestamp
            .checked_add((skip as u64 * 1_000_000 / u64::from(self.rate)) as i64)
            .ok_or("Opus presentation timestamp overflows")?;
        let bytes = pcm[skip * self.channels..]
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        Ok(vec![Output {
            bytes,
            format: "f32",
            timestamp,
            duration: (frames - skip) as u64 * 1_000_000 / u64::from(self.rate),
            frames: (frames - skip) as u32,
            description: None,
        }])
    }

    pub(super) fn flush(&mut self) -> Vec<Output> {
        // There are no delayed packet-decoder outputs. After flush a key chunk
        // is required by the API, but it must not reapply an OpusHead pre-skip.
        Vec::new()
    }
}
