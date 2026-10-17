//! Native gesture ownership is decided at input receipt, not delayed renderer reply.
// Smooth scrolling may be aborted by the user (CSSOM View scrolling). This fence
// abandons prior animation, not wheel dispatch or the new event's default-action gate.
// https://drafts.csswg.org/cssom-view/#smooth-scroll
use better_web_browser::renderer_protocol::{RuntimeReport, WheelDecision};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::windows_app) enum WheelAdjustment {
    Default(f32),
    Correction(i32),
}

struct PreappliedWheel {
    requested_delta: f32,
    requested_pixels: i32,
    distance_pixels: i32,
    scale: f32,
}

impl PreappliedWheel {
    fn absorb(&mut self, newer: Self) {
        self.requested_delta += newer.requested_delta;
        self.requested_pixels = self.requested_pixels.saturating_add(newer.requested_pixels);
        self.distance_pixels = self.distance_pixels.saturating_add(newer.distance_pixels);
        self.scale = newer.scale;
    }
}

#[derive(Default)]
pub(in crate::windows_app) struct WheelGesture {
    direction: i8,
    first_live_sequence: u64,
    blocking_listeners: Option<bool>,
    nested_scroll: Option<bool>,
    preapplied: HashMap<u64, PreappliedWheel>,
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
        self.preapplied.clear();
    }

    pub(in crate::windows_app) fn can_preapply(&self) -> bool {
        self.blocking_listeners == Some(false) && self.nested_scroll == Some(false)
    }

    pub(in crate::windows_app) fn preapply(
        &mut self,
        sequence: u64,
        delta: f32,
        requested_pixels: i32,
        distance_pixels: i32,
        scale: f32,
    ) -> bool {
        if !self.can_preapply()
            || sequence == 0
            || sequence < self.first_live_sequence
            || !delta.is_finite()
            || delta == 0.0
            || distance_pixels == 0
            || !scale.is_finite()
            || scale <= 0.0
        {
            return false;
        }
        let wheel = PreappliedWheel {
            requested_delta: delta,
            requested_pixels,
            distance_pixels,
            scale,
        };
        match self.preapplied.get_mut(&sequence) {
            Some(retired) => retired.absorb(wheel),
            None => {
                self.preapplied.insert(sequence, wheel);
            }
        }
        true
    }

    /// Moves an unsent wheel's preapplied distance to the input that absorbed it.
    /// The merged verdict then reconciles both contributions, added in queue order.
    pub(in crate::windows_app) fn coalesce(&mut self, retired: u64, sequence: u64) {
        if let Some(retired) = self.preapplied.remove(&retired) {
            self.preapplied.insert(sequence, retired);
        }
    }

    pub(in crate::windows_app) fn reconcile(
        &mut self,
        report: &RuntimeReport,
    ) -> Vec<WheelAdjustment> {
        if let Some(blocking) = report.wheel_has_blocking_listeners {
            self.blocking_listeners = Some(blocking);
        }
        if let Some(nested) = report.wheel_has_nested_scroll {
            self.nested_scroll = Some(nested);
        }
        if report.wheel_acknowledgements.is_empty() {
            return (report.viewport_wheel_delta_y != 0.0)
                .then_some(WheelAdjustment::Default(report.viewport_wheel_delta_y))
                .into_iter()
                .collect();
        }
        // Keep DOM delivery, nested snapshots and script scrolls untouched. Only
        // obsolete shell-owned smooth-scroll distance is abandoned. Per-verdict
        // contributions allow compaction to retain old and new input together.
        report
            .wheel_acknowledgements
            .iter()
            .flat_map(|ack| {
                let preapplied = self.preapplied.remove(&ack.sequence);
                if ack.sequence < self.first_live_sequence {
                    return Vec::new();
                }
                let expected = if ack.decision == WheelDecision::Viewport {
                    ack.viewport_delta_y
                } else {
                    0.0
                };
                // An absolute script scroll replaces the speculative target.
                if report.viewport_scroll_y.is_some() {
                    (expected != 0.0)
                        .then_some(WheelAdjustment::Default(expected))
                        .into_iter()
                        .collect()
                } else if let Some(preapplied) = preapplied {
                    let expected_pixels = if expected == preapplied.requested_delta {
                        preapplied.requested_pixels
                    } else {
                        (f64::from(expected) * f64::from(preapplied.scale))
                            .round()
                            .clamp(i32::MIN as f64, i32::MAX as f64) as i32
                    };
                    (expected_pixels != preapplied.distance_pixels)
                        .then_some(WheelAdjustment::Correction(
                            expected_pixels.saturating_sub(preapplied.distance_pixels),
                        ))
                        .into_iter()
                        .collect()
                } else {
                    (expected != 0.0)
                        .then_some(WheelAdjustment::Default(expected))
                        .into_iter()
                        .collect()
                }
            })
            .collect()
    }

    #[cfg(test)]
    pub(in crate::windows_app) fn viewport_delta(&mut self, report: &RuntimeReport) -> f32 {
        self.reconcile(report)
            .into_iter()
            .fold(0.0_f64, |sum, adjustment| {
                sum + f64::from(match adjustment {
                    WheelAdjustment::Default(delta) => delta,
                    WheelAdjustment::Correction(pixels) => pixels as f32,
                })
            })
            .clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32
    }
}

#[cfg(test)]
mod tests;
