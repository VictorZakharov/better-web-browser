//! Linear PCM WebCodecs registration: interleaved complete sample frames.
//! This is byte layout admission, not a new codec implementation. Native sample
//! types pass through unchanged; 24-bit samples are left-aligned in signed i32.
use super::{Output, config::Config};

pub(super) fn supported(codec: &str) -> bool {
    matches!(
        codec,
        "pcm-u8" | "pcm-s16" | "pcm-s24" | "pcm-s32" | "pcm-f32"
    )
}

pub(super) struct Decoder {
    format: &'static str,
    rate: u32,
    channels: usize,
    sample_bytes: usize,
    expand_24: bool,
}

impl Decoder {
    pub(super) fn new(config: &Config) -> Result<Self, String> {
        config.validate(false)?;
        let (format, sample_bytes, expand_24) = match config.codec.as_str() {
            "pcm-u8" => ("u8", 1, false),
            "pcm-s16" => ("s16", 2, false),
            "pcm-s24" => ("s32", 3, true),
            "pcm-s32" => ("s32", 4, false),
            "pcm-f32" => ("f32", 4, false),
            _ => return Err("unsupported Linear PCM sample type".into()),
        };
        Ok(Self {
            format,
            rate: config.sample_rate,
            channels: config.number_of_channels as usize,
            sample_bytes,
            expand_24,
        })
    }

    pub(super) fn decode(&self, bytes: &[u8], timestamp: i64) -> Result<Vec<Output>, String> {
        let stride = self.sample_bytes * self.channels;
        if bytes.is_empty()
            || bytes.len() > super::MAX_INPUT_BYTES
            || !bytes.len().is_multiple_of(stride)
        {
            return Err("Linear PCM chunk must contain bounded complete sample frames".into());
        }
        let frames = bytes.len() / stride;
        let bytes = if self.expand_24 {
            // Sign is retained in bit 31. Multiplication or floating conversion
            // here would unnecessarily lose exact 24-bit PCM transport.
            bytes
                .chunks_exact(3)
                .flat_map(|sample| [0, sample[0], sample[1], sample[2]])
                .collect()
        } else {
            bytes.to_vec()
        };
        Ok(vec![Output {
            bytes,
            format: self.format,
            timestamp,
            frames: frames as u32,
            sample_rate: self.rate,
            channels: self.channels as u32,
            duration: frames as u64 * 1_000_000 / u64::from(self.rate),
            description: None,
        }])
    }
}

#[cfg(test)]
mod tests;
