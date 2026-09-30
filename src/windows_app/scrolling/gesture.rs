//! Native gesture ownership is decided at input receipt, not delayed renderer reply.
// Smooth scrolling may be aborted by the user (CSSOM View scrolling). This fence
// abandons prior animation, not wheel dispatch or the new event's default-action gate.
// https://drafts.csswg.org/cssom-view/#smooth-scroll
use better_web_browser::renderer_protocol::{RuntimeReport, WheelDecision};

#[derive(Default)]
pub(in crate::windows_app) struct WheelGesture {
    direction: i8,
    first_live_sequence: u64,
}

impl WheelGesture {
    /// Every physical reversal retires older viewport defaults, even if this input
    /// is later cancelled by a listener, consumed by a nested box, or rejected.
    pub(in crate::windows_app) fn observe(&mut self, sequence: u64, delta: f32) -> bool {
        if sequence == 0 || !delta.is_finite() || delta == 0.0 {
            return false;
        }
        let direction = if delta > 0.0 { 1 } else { -1 };
        let reversed = self.direction != 0 && self.direction != direction;
        if self.direction == 0 || reversed {
            self.first_live_sequence = self.first_live_sequence.max(sequence);
        }
        self.direction = direction;
        reversed
    }

    pub(in crate::windows_app) fn first_live_sequence(&self) -> u64 {
        self.first_live_sequence
    }

    pub(in crate::windows_app) fn retire_before(&mut self, sequence: u64) {
        self.first_live_sequence = self.first_live_sequence.max(sequence);
        self.direction = 0;
    }

    pub(in crate::windows_app) fn viewport_delta(&self, report: &RuntimeReport) -> f32 {
        if report.wheel_acknowledgements.is_empty() {
            return report.viewport_wheel_delta_y;
        }
        // Keep DOM delivery, nested snapshots and script scrolls untouched. Only
        // obsolete shell-owned smooth-scroll distance is abandoned. Per-verdict
        // contributions allow compaction to retain old and new input together.
        report
            .wheel_acknowledgements
            .iter()
            .filter(|ack| {
                ack.sequence >= self.first_live_sequence && ack.decision == WheelDecision::Viewport
            })
            .fold(0.0_f64, |sum, ack| sum + f64::from(ack.viewport_delta_y))
            .clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32
    }
}

#[cfg(test)]
mod tests;
