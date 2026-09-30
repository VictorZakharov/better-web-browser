//! Reject timing arithmetic that the upstream MKV reader treats as clean EOF.
//! This inspects metadata and the fixed Block prefix, not lacing or codec data.
//! https://www.matroska.org/technical/elements.html

use std::num::NonZeroU32;
use symphonia::core::units::TimeBase;

#[derive(Default)]
pub(super) struct Timing {
    info: bool,
    timestamp_scale: Option<u64>,
    track_scale: Option<f64>,
    codec_delay: Option<u64>,
    default_duration: Option<u64>,
    min_cluster: Option<u64>,
    max_cluster: u64,
    min_relative: i16,
    max_relative: i16,
    max_block_duration: u64,
    max_lace_frames: u64,
}

impl Timing {
    pub(super) fn metadata(&mut self, id: u32, parent: u32, bytes: &[u8]) -> Result<(), String> {
        match (id, parent) {
            (0x1549_a966, super::SEGMENT) => {
                if self.info {
                    return Err("WebM Info is repeated".into());
                }
                self.info = true;
            }
            (0x1549_a966, _) => return Err("WebM Info is outside Segment".into()),
            (0x2ad7b1, 0x1549_a966) => {
                let value = super::uint(bytes)?;
                if value == 0 || self.timestamp_scale.replace(value).is_some() {
                    return Err("WebM TimestampScale is zero or repeated".into());
                }
            }
            (0x23314f, super::TRACK) => {
                let value = float(bytes)?;
                if !value.is_finite() || value <= 0.0 || self.track_scale.replace(value).is_some() {
                    return Err("WebM TrackTimestampScale is invalid or repeated".into());
                }
            }
            (0x56aa, super::TRACK) => {
                let previous = self.codec_delay.replace(super::uint(bytes)?);
                if previous.is_some() {
                    return Err("WebM CodecDelay is repeated".into());
                }
            }
            (0x23e383, super::TRACK) => {
                let value = super::uint(bytes)?;
                if value == 0 || self.default_duration.replace(value).is_some() {
                    return Err("WebM DefaultDuration is zero or repeated".into());
                }
            }
            (0xe7, super::CLUSTER) => {
                let value = super::uint(bytes)?;
                self.min_cluster = Some(self.min_cluster.map_or(value, |old| old.min(value)));
                self.max_cluster = self.max_cluster.max(value);
            }
            (0x9b, 0xa0) => {
                self.max_block_duration = self.max_block_duration.max(super::uint(bytes)?);
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn block(&mut self, bytes: &[u8]) -> Result<(), String> {
        let (_, width) = super::vint(bytes, false)?; // TrackNumber, not a lace size.
        let prefix = bytes
            .get(width..width + 3)
            .ok_or("WebM fixed Block header is truncated")?;
        let relative = i16::from_be_bytes([prefix[0], prefix[1]]);
        self.min_relative = self.min_relative.min(relative);
        self.max_relative = self.max_relative.max(relative);
        let frames = if prefix[2] & 6 == 0 {
            1
        } else {
            u64::from(*bytes.get(width + 3).ok_or("WebM lace count is missing")?) + 1
        };
        self.max_lace_frames = self.max_lace_frames.max(frames);
        Ok(())
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        let scale = self.timestamp_scale.unwrap_or(1_000_000);
        let numerator = u32::try_from(scale)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or("WebM TimestampScale exceeds the upstream timebase")?;
        let track_scale = self.track_scale.unwrap_or(1.0);
        let base = TimeBase::new(numerator, NonZeroU32::new(1_000_000_000).unwrap())
            .reduce()
            .scale(track_scale)
            .ok_or("WebM scaled track timebase is unsupported")?;
        // Reuse the upstream timebase. Its ns-to-track-tick divisor must be
        // nonzero, and every signed PTS / lace endpoint must fit before decode.
        let divisor =
            (1_000_000_000_u64 * u64::from(base.numer.get())) / u64::from(base.denom.get());
        if divisor == 0 {
            return Err("WebM track timebase cannot represent nanosecond durations".into());
        }
        let delay = self.codec_delay.unwrap_or(0) / divisor;
        let default_ticks = self
            .default_duration
            .unwrap_or(0)
            .checked_mul(self.max_lace_frames)
            .ok_or("WebM laced DefaultDuration overflows")?
            / divisor;
        let duration = self.max_block_duration.max(default_ticks);
        let first = scaled_ticks(self.min_cluster.unwrap_or(0), track_scale)?;
        let last = scaled_ticks(self.max_cluster, track_scale)?;
        // The reader adds the relative offset before subtracting CodecDelay;
        // a later subtraction cannot rescue an overflowing intermediate PTS.
        last.checked_add(i64::from(self.max_relative))
            .ok_or("WebM relative Block timestamp overflows")?;
        duration
            .checked_add(self.max_lace_frames.saturating_sub(1))
            .ok_or("WebM laced duration accumulator overflows")?;
        // A whole-file extrema envelope deliberately avoids storing or parsing
        // every lace. Separate near-i64-limit extremes may be rejected even
        // when individually representable; ordinary bounded timelines are not.
        let minimum = i128::from(first) + i128::from(self.min_relative) - i128::from(delay);
        let maximum = i128::from(last) + i128::from(self.max_relative) - i128::from(delay)
            + i128::from(duration);
        if minimum < i128::from(i64::MIN) || maximum > i128::from(i64::MAX) {
            return Err("WebM block timestamp or laced endpoint overflows".into());
        }
        Ok(())
    }
}

fn scaled_ticks(value: u64, factor: f64) -> Result<i64, String> {
    if factor == 1.0 {
        return i64::try_from(value).map_err(|_| "WebM Cluster Timestamp overflows".into());
    }
    // Match upstream's rounding without its saturating float-to-u64 cast.
    // 2^63 is exactly representable; i64::MAX as f64 rounds up to that value.
    let ticks = (value as f64 * factor).round();
    if !ticks.is_finite() || !(0.0..9_223_372_036_854_775_808.0).contains(&ticks) {
        return Err("WebM scaled Cluster Timestamp overflows".into());
    }
    Ok(ticks as i64)
}

fn float(bytes: &[u8]) -> Result<f64, String> {
    match bytes.len() {
        0 => Ok(1.0), // The schema default for an empty TrackTimestampScale.
        4 => Ok(f64::from(f32::from_be_bytes(bytes.try_into().unwrap()))),
        8 => Ok(f64::from_be_bytes(bytes.try_into().unwrap())),
        _ => Err("WebM floating-point metadata width is invalid".into()),
    }
}
