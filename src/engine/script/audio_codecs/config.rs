//! Packet codec admission. These limits are checked again in the native worker;
//! JavaScript's support-query result is not trusted as a resource authorization.
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Config {
    pub(super) codec: String,
    pub(super) sample_rate: u32,
    pub(super) number_of_channels: u32,
    #[serde(default)]
    pub(super) bitrate: Option<u32>,
    #[serde(default)]
    pub(super) bitrate_mode: Option<String>,
    #[serde(default)]
    pub(super) description: Option<Vec<u8>>,
    #[serde(default)]
    pub(super) opus: Option<OpusOptions>,
    #[serde(default)]
    pub(super) flac: Option<FlacOptions>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FlacOptions {
    pub(super) block_size: u32,
    pub(super) compress_level: u32,
}

impl Default for FlacOptions {
    fn default() -> Self {
        Self {
            block_size: 0,
            compress_level: 5,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OpusOptions {
    pub(super) format: String,
    pub(super) application: String,
    pub(super) signal: String,
    pub(super) frame_duration: u32,
    pub(super) complexity: u32,
    pub(super) packetlossperc: u32,
    pub(super) useinbandfec: bool,
    pub(super) usedtx: bool,
}

impl Default for OpusOptions {
    fn default() -> Self {
        Self {
            format: "opus".into(),
            application: "audio".into(),
            signal: "auto".into(),
            frame_duration: 20_000,
            complexity: 9,
            packetlossperc: 0,
            useinbandfec: false,
            usedtx: false,
        }
    }
}

impl Config {
    pub(super) fn read(json: &str, encode: bool) -> Result<Self, String> {
        // JSON represents each extradata byte with up to four characters plus
        // separators. The wire budget must accommodate the 64 KiB byte budget.
        if json.len() > 384 * 1024 {
            return Err("audio codec configuration exceeds its metadata budget".into());
        }
        let config: Self =
            serde_json::from_str(json).map_err(|_| "invalid native audio codec configuration")?;
        config.validate(encode)?;
        Ok(config)
    }

    pub(super) fn validate(&self, encode: bool) -> Result<(), String> {
        if self
            .description
            .as_ref()
            .is_some_and(|bytes| bytes.len() > 65_536)
        {
            return Err("audio codec description exceeds 64 KiB".into());
        }
        if self.codec == "flac" && encode {
            return super::flac_encoder::validate(self);
        }
        if self.flac.is_some() {
            return Err("FLAC encoder options require a FLAC encoder".into());
        }
        if super::compressed::supported(&self.codec) {
            return super::compressed::validate(self, encode);
        }
        if super::pcm::supported(&self.codec) {
            if encode
                || self.description.is_some()
                || self.opus.is_some()
                || self.bitrate.is_some()
                || self.bitrate_mode.is_some()
                || !(1..=384_000).contains(&self.sample_rate)
                || !(1..=32).contains(&self.number_of_channels)
            {
                return Err(
                    "Linear PCM requires decoder-only 1–384 kHz, 1–32 channels, without extradata"
                        .into(),
                );
            }
            return Ok(());
        }
        if self.codec != "opus"
            || !matches!(self.sample_rate, 8_000 | 12_000 | 16_000 | 24_000 | 48_000)
            || !matches!(self.number_of_channels, 1 | 2)
        {
            return Err(
                "Opus requires a native 8/12/16/24/48 kHz mono/stereo configuration".into(),
            );
        }
        if encode {
            if self.description.is_some()
                || self
                    .bitrate
                    .is_some_and(|rate| !(6_000..=510_000).contains(&rate))
                || self
                    .bitrate_mode
                    .as_deref()
                    .is_some_and(|mode| !matches!(mode, "constant" | "variable"))
            {
                return Err("unsupported Opus encoder configuration".into());
            }
            if let Some(options) = &self.opus
                && (!matches!(options.format.as_str(), "opus" | "ogg")
                    || !matches!(options.application.as_str(), "audio" | "voip" | "lowdelay")
                    || !matches!(options.signal.as_str(), "auto" | "music" | "voice")
                    || !(2_500..=120_000).contains(&options.frame_duration)
                    || !options.frame_duration.is_multiple_of(2_500)
                    || options.complexity > 10
                    || options.packetlossperc > 100)
            {
                return Err("unsupported Opus packet encoder options".into());
            }
        } else {
            if self.bitrate.is_some() || self.bitrate_mode.is_some() || self.opus.is_some() {
                return Err("decoder configuration contains encoder-only members".into());
            }
            if let Some(bytes) = &self.description {
                let head = crate::webm_opus::header::read(bytes)?;
                if u32::from(head.channels) != self.number_of_channels {
                    return Err("OpusHead channels disagree with the decoder configuration".into());
                }
            }
        }
        Ok(())
    }

    pub(super) fn channels(&self) -> opus::Channels {
        if self.number_of_channels == 1 {
            opus::Channels::Mono
        } else {
            opus::Channels::Stereo
        }
    }
}
