//! Registered elementary audio packets, using the existing pure-Rust decoders.
//! Container files are not accepted as EncodedAudioChunk payloads. In particular,
//! AAC without description means ADTS, not an undocumented raw-AAC fallback.
mod aac;
mod flac;
mod mp3;
#[cfg(test)]
mod tests;
mod vorbis;

use super::{Output, config::Config};
use symphonia::core::codecs::audio::well_known::{CODEC_ID_AAC, CODEC_ID_MP3};
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions};
use symphonia::core::packet::PacketRef;
use symphonia::core::units::{Duration, Timestamp};

pub(super) fn supported(codec: &str) -> bool {
    matches!(
        codec,
        "mp3" | "mp4a.69" | "mp4a.6B" | "mp4a.40.2" | "mp4a.40.02" | "mp4a.67" | "flac" | "vorbis"
    )
}

pub(super) fn validate(config: &Config, encode: bool) -> Result<(), String> {
    if encode {
        return Err("this elementary audio codec has no encoder".into());
    }
    if config.sample_rate == 0
        || config.number_of_channels == 0
        || config.bitrate.is_some()
        || config.bitrate_mode.is_some()
        || config.opus.is_some()
    {
        return Err("invalid elementary audio decoder configuration".into());
    }
    // The registrations ignore the requested dimensions. Only stream metadata
    // controls allocations; a nominal 1 Hz/32-channel config is not a resampler.
    match config.codec.as_str() {
        "mp3" | "mp4a.69" | "mp4a.6B" => Ok(()), // MP3 description is unused.
        "flac" => flac::parameters(config.description.as_deref()).map(|_| ()),
        "vorbis" => vorbis::parameters(config.description.as_deref()).map(|_| ()),
        _ => {
            if let Some(bytes) = &config.description {
                aac::parameters(bytes).map(|_| ())
            } else {
                Ok(())
            }
        }
    }
}

enum Framing {
    Mp3 { first: bool, skip: usize },
    RawAac,
    Vorbis,
    Adts { dimensions: Option<(u32, u32)> },
    Flac { description: Vec<u8> },
}

pub(super) struct Decoder {
    codec: Option<Box<dyn AudioDecoder>>,
    framing: Framing,
}

fn decoder(parameters: &AudioCodecParameters) -> Result<Box<dyn AudioDecoder>, String> {
    symphonia::default::get_codecs()
        .make_audio_decoder(parameters, &AudioDecoderOptions::default())
        .map_err(|error| format!("create elementary audio decoder: {error}"))
}

impl Decoder {
    pub(super) fn new(config: &Config) -> Result<Self, String> {
        validate(config, false)?;
        let (framing, parameters) = match config.codec.as_str() {
            "mp3" | "mp4a.69" | "mp4a.6B" => {
                let mut params = AudioCodecParameters::new();
                params.for_codec(CODEC_ID_MP3);
                (
                    Framing::Mp3 {
                        first: true,
                        skip: 0,
                    },
                    Some(params),
                )
            }
            "flac" => {
                let description = config
                    .description
                    .clone()
                    .ok_or("FLAC STREAMINFO is required")?;
                (Framing::Flac { description }, None)
            }
            "vorbis" => (
                Framing::Vorbis,
                Some(vorbis::parameters(config.description.as_deref())?),
            ),
            _ => match &config.description {
                Some(bytes) => (Framing::RawAac, Some(aac::parameters(bytes)?)),
                None => (Framing::Adts { dimensions: None }, None),
            },
        };
        Ok(Self {
            codec: parameters.as_ref().map(decoder).transpose()?,
            framing,
        })
    }

    pub(super) fn decode(&mut self, bytes: &[u8], timestamp: i64) -> Result<Vec<Output>, String> {
        if bytes.is_empty() || bytes.len() > super::MAX_INPUT_BYTES {
            return Err("elementary audio packet is empty or exceeds 512 KiB".into());
        }
        if let Framing::Flac { description } = &self.framing {
            return flac::decode(description, bytes, timestamp);
        }
        let mut skip = 0;
        let payload = match &mut self.framing {
            Framing::Mp3 {
                first,
                skip: pending,
            } => {
                let header = mp3::header(bytes)?;
                if *first {
                    *pending = mp3::priming(bytes, &header)?;
                    *first = false;
                }
                skip = *pending;
                bytes
            }
            Framing::RawAac => {
                if bytes[0] >> 5 == 7 {
                    return Err("AAC raw_data_block terminates before any audio element".into());
                }
                bytes
            }
            Framing::Vorbis => {
                if bytes[0] & 1 != 0 {
                    return Err("Vorbis chunk requires an audio packet, not a header packet".into());
                }
                bytes
            }
            Framing::Adts { dimensions } => {
                let frame = aac::adts(bytes)?;
                let next = (frame.rate, frame.channels);
                if dimensions.is_some_and(|old| old != next) {
                    return Err("ADTS dimensions changed; configure a new decoder".into());
                }
                if dimensions.is_none() {
                    let mut parameters = AudioCodecParameters::new();
                    parameters
                        .for_codec(CODEC_ID_AAC)
                        .with_sample_rate(frame.rate)
                        .with_channels(symphonia::core::audio::Channels::Discrete(
                            frame.channels as u16,
                        ));
                    self.codec = Some(decoder(&parameters)?);
                    *dimensions = Some(next);
                }
                frame.payload
            }
            Framing::Flac { description } => {
                return flac::decode(description, bytes, timestamp);
            }
        };
        let packet = PacketRef::new(0, Timestamp::ZERO, Duration::ZERO, payload);
        let decoded = self
            .codec
            .as_mut()
            .ok_or("audio decoder is not initialized")?
            .decode_ref(&packet)
            .map_err(|error| format!("decode elementary audio packet: {error}"))?;
        let rate = decoded.spec().rate();
        let channels = decoded.num_planes();
        let frames = decoded.frames();
        if !(1..=384_000).contains(&rate)
            || !(1..=8).contains(&channels)
            || frames > 65_535
            || frames * channels * 4 > 2 * 1024 * 1024
        {
            return Err("decoded audio dimensions exceed the packet output budget".into());
        }
        let skipped = skip.min(frames);
        if let Framing::Mp3 { skip, .. } = &mut self.framing {
            *skip -= skipped;
        }
        if frames == skipped {
            return Ok(Vec::new());
        }
        let mut planes = Vec::<Vec<f32>>::new();
        decoded.copy_to_vecs_planar(&mut planes);
        if planes.iter().flatten().any(|sample| !sample.is_finite()) {
            return Err("audio decoder returned non-finite PCM".into());
        }
        let bytes = planes
            .iter()
            .flat_map(|plane| {
                plane[skipped..]
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
            })
            .collect();
        Ok(vec![Output {
            bytes,
            format: "f32-planar",
            timestamp: timestamp
                .checked_add((skipped as u64 * 1_000_000 / u64::from(rate)) as i64)
                .ok_or("decoded audio timestamp overflows")?,
            duration: (frames - skipped) as u64 * 1_000_000 / u64::from(rate),
            frames: (frames - skipped) as u32,
            sample_rate: rate,
            channels: channels as u32,
            description: None,
        }])
    }
}
