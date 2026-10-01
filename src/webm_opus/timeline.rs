//! Convert Matroska ticks to sample positions without cumulative rounding drift.

use super::metadata::Block;

pub(super) struct Window {
    pub(super) left: u64,
    pub(super) right: u64,
    pub(super) gap: u64,
}

#[derive(Default)]
pub(super) struct Timeline {
    raw_end: Option<i64>,
    pub(super) presented: u64,
}

impl Timeline {
    pub(super) fn block(
        &mut self,
        block: &Block,
        scale_ns: u64,
        pre_skip: u16,
        raw_frames: u64,
    ) -> Result<Window, String> {
        let ns = block
            .time_ticks
            .checked_mul(i128::from(scale_ns))
            .ok_or("WebM Opus block timestamp overflows")?;
        let wanted = samples(ns)?
            .checked_sub(i64::from(pre_skip))
            .ok_or("WebM Opus pre-skip timestamp overflows")?;
        // Integer Block timestamps quantize each packet boundary. Snap only
        // within one timestamp tick; larger gaps retain timed silence, while
        // overlapping/reordered blocks are rejected rather than concatenated.
        let tolerance = (u128::from(scale_ns) * 48_000).div_ceil(1_000_000_000) as u64 + 1;
        let raw_start = match self.raw_end {
            Some(end) if wanted.abs_diff(end) <= tolerance => end,
            Some(end) if wanted < end => {
                return Err("WebM Opus blocks overlap or run backwards".into());
            }
            _ => wanted,
        };
        let raw_end = raw_start
            .checked_add(
                i64::try_from(raw_frames).map_err(|_| "WebM Opus block duration overflows")?,
            )
            .ok_or("WebM Opus block endpoint overflows")?;
        let padding = samples(i128::from(block.padding_ns))?.unsigned_abs();
        if padding > raw_frames {
            return Err("WebM Opus DiscardPadding exceeds its block".into());
        }
        let trim_left = if block.padding_ns < 0 { padding } else { 0 };
        let trim_right = if block.padding_ns > 0 { padding } else { 0 };
        let left = trim_left
            .max(raw_start.unsigned_abs() * u64::from(raw_start < 0))
            .min(raw_frames);
        let right = (raw_frames - trim_right).max(left);
        // DiscardPadding removes time from the presented track. Do not turn
        // discarded prefix/tail samples back into silence, either in this Block
        // or when the next Block's raw timestamp follows its padded endpoint.
        let previous_end = self.raw_end.unwrap_or(0).max(0) as u64;
        let gap = (raw_start.max(0) as u64).saturating_sub(previous_end);
        self.presented = self
            .presented
            .checked_add(gap)
            .and_then(|n| n.checked_add(right - left))
            .ok_or("WebM Opus presentation duration overflows")?;
        self.raw_end = Some(raw_end);
        Ok(Window { left, right, gap })
    }
}

pub(super) fn samples(ns: i128) -> Result<i64, String> {
    let scaled = ns
        .checked_mul(48_000)
        .ok_or("WebM Opus nanosecond conversion overflows")?;
    // Round once at the sample boundary, not once for each decoded packet.
    let quotient = scaled / 1_000_000_000;
    let remainder = scaled % 1_000_000_000;
    let value = quotient + i128::from(remainder.unsigned_abs() >= 500_000_000) * scaled.signum();
    i64::try_from(value).map_err(|_| "WebM Opus sample timestamp exceeds bounds".into())
}
