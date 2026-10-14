//! Display-rate arithmetic shared by the native embedding and animation scheduler.

use std::time::Duration;

/// A refresh frequency in cycles per second. Keep the OS rational: 60000/1001
/// must not become 59 Hz, and high-refresh displays have no browser FPS ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefreshRate {
    numerator: u32,
    denominator: u32,
}

impl Default for RefreshRate {
    fn default() -> Self {
        Self::FALLBACK
    }
}

impl RefreshRate {
    /// Used only when the embedding cannot obtain an active display frequency.
    pub const FALLBACK: Self = Self {
        numerator: 60,
        denominator: 1,
    };

    pub const fn new(numerator: u32, denominator: u32) -> Option<Self> {
        if numerator == 0 || denominator == 0 {
            return None;
        }
        Some(Self {
            numerator,
            denominator,
        })
    }

    pub const fn numerator(self) -> u32 {
        self.numerator
    }
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// The first grid point strictly after `elapsed`. Compute from the original
    /// rational each time, rather than accumulating a rounded frame interval.
    pub fn next_after(self, elapsed: Duration) -> Duration {
        let period = u128::from(self.denominator) * 1_000_000_000;
        let frequency = u128::from(self.numerator);
        let index = elapsed.as_nanos().saturating_mul(frequency) / period + 1;
        duration_from_nanos(index.saturating_mul(period).div_ceil(frequency))
    }
}

fn duration_from_nanos(nanos: u128) -> Duration {
    let seconds = nanos / 1_000_000_000;
    if seconds > u128::from(u64::MAX) {
        return Duration::MAX;
    }
    Duration::new(seconds as u64, (nanos % 1_000_000_000) as u32)
}

#[cfg(test)]
mod tests;
