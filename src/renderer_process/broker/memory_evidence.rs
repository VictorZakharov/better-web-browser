//! Retain observed commit peaks when a renderer exits or an OS query fails.
use crate::renderer_process::windows::ProcessSample;

/// Fixed-size broker-side evidence, not exact peak private commit or driver VRAM.
/// Sampling is independent of the author event loop, at the broker's 1s cadence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RendererMemoryEvidence {
    pub current_sample_available: bool,
    pub successful_samples: u64,
    pub last_private_bytes: Option<usize>,
    pub last_working_set_bytes: Option<usize>,
    pub observed_peak_private_bytes: Option<usize>,
    pub retained_peak_working_set_bytes: Option<usize>,
}

impl RendererMemoryEvidence {
    pub(super) fn from_sample(sample: ProcessSample) -> Self {
        let mut evidence = Self::default();
        evidence.observe(sample);
        evidence
    }

    pub(super) fn observe(&mut self, sample: ProcessSample) {
        if !sample.memory_available {
            self.unavailable();
            return;
        }
        self.current_sample_available = true;
        self.successful_samples = self.successful_samples.saturating_add(1);
        self.last_private_bytes = Some(sample.private_memory);
        self.last_working_set_bytes = Some(sample.working_set);
        self.observed_peak_private_bytes = Some(
            self.observed_peak_private_bytes
                .unwrap_or(0)
                .max(sample.private_memory),
        );
        self.retained_peak_working_set_bytes = Some(
            self.retained_peak_working_set_bytes
                .unwrap_or(0)
                .max(sample.peak_working_set)
                .max(sample.working_set),
        );
    }

    pub(super) fn unavailable(&mut self) {
        self.current_sample_available = false;
        // Keep the last successful observation and high-water evidence, but
        // never present a stale sample as current or failure as recovery to zero.
    }
}

#[cfg(test)]
mod tests;
