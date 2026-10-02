//! Honest software AV1 baseline admission, independent of author support probes.
use serde::Deserialize;
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Config {
    pub codec: String,
    pub coded_width: Option<u32>,
    pub coded_height: Option<u32>,
    pub display_aspect_width: Option<u32>,
    pub display_aspect_height: Option<u32>,
    pub hardware_acceleration: String,
    pub optimize_for_latency: bool,
    pub description: Option<Vec<u8>>,
    pub color_space: Option<serde_json::Value>,
    pub rotation: f64,
    pub flip: bool,
}
impl Config {
    pub(super) fn read(json: &str) -> Result<Self, String> {
        if json.len() > 384 * 1024 {
            return Err("video configuration exceeds its metadata budget".into());
        }
        let value: Self =
            serde_json::from_str(json).map_err(|_| "invalid native video configuration")?;
        value.validate()?;
        Ok(value)
    }
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.color_space.is_some() {
            return Err(
                "AV1 color-space overrides are not implemented; use stream metadata".into(),
            );
        }
        if !av1_codec(&self.codec) {
            return Err("only AV1 profile 0, main tier, 8-bit is implemented".into());
        }
        if self.hardware_acceleration == "prefer-hardware"
            || !matches!(
                self.hardware_acceleration.as_str(),
                "no-preference" | "prefer-software"
            )
        {
            return Err("AV1 decoder is software-only".into());
        }
        if self
            .description
            .as_ref()
            .is_some_and(|bytes| bytes.len() > 65536)
        {
            return Err("video description exceeds 64 KiB".into());
        }
        for (width, height, dimensions) in [
            (self.coded_width, self.coded_height, true),
            (self.display_aspect_width, self.display_aspect_height, false),
        ] {
            if width.is_some() != height.is_some() || width == Some(0) || height == Some(0) {
                return Err("video dimensions require a nonzero pair".into());
            }
            if dimensions
                && width.zip(height).is_some_and(|(w, h)| {
                    w > 1920 || h > 1920 || u64::from(w) * u64::from(h) > 1920 * 1080
                })
            {
                return Err("AV1 baseline exceeds its 1080p pixel budget".into());
            }
        }
        if !self.rotation.is_finite() {
            return Err("video rotation must be finite".into());
        }
        // Latency is a hint. The single-thread backend retains only the references
        // required by AV1; it does not claim a hardware low-latency pipeline.
        let _ = self.optimize_for_latency;
        let _ = self.flip;
        Ok(())
    }
}

fn av1_codec(codec: &str) -> bool {
    let parts: Vec<_> = codec.split('.').collect();
    if !matches!(parts.len(), 4 | 10) || parts[0] != "av01" || parts[1] != "0" || parts[3] != "08" {
        return false;
    }
    let level = parts[2].as_bytes();
    if level.len() != 3 || level[2] != b'M' || !level[..2].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let number = (level[0] - b'0') * 10 + (level[1] - b'0');
    if number > 23 {
        return false;
    }
    if parts.len() == 10 {
        if !matches!(parts[4], "0" | "1") || !matches!(parts[5], "110" | "111" | "112") {
            return false;
        }
        // Only SDR matrices implemented by the existing AV1 pixel conversion.
        if !matches!(
            parts[6],
            "01" | "02" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12"
        ) || !matches!(
            parts[7],
            "01" | "02" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12" | "13"
        ) || !matches!(parts[8], "01" | "02" | "04" | "05" | "06" | "07" | "09")
            || !matches!(parts[9], "0" | "1")
        {
            return false;
        }
    }
    true
}
