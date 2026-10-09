//! Coalesce verified color/underline input painting at a bounded rendering opportunity.
//! DOM input dispatch and its side effects are never delayed by this gate.
//! https://html.spec.whatwg.org/multipage/webappapis.html#update-the-rendering

use super::{DocumentRuntime, ScriptOutcome, StyleRefreshStats, micros};
use crate::engine::dom::NodeId;
use crate::engine::invalidation::{InvalidationImpact, RenderInvalidation};
use std::time::{Duration, Instant};

const COLOR_PAINT_INTERVAL: Duration = Duration::from_millis(16);

pub(super) struct PendingColorPaint {
    invalidation: RenderInvalidation,
    deadline: Option<Instant>,
    clock_anchor: Instant,
}

impl Default for PendingColorPaint {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl PendingColorPaint {
    fn new(now: Instant) -> Self {
        Self {
            invalidation: RenderInvalidation::default(),
            deadline: None,
            clock_anchor: now,
        }
    }

    pub(super) fn clock_advanced(&mut self, now: Instant) {
        self.clock_anchor = now;
    }

    fn defer(&mut self, outcome: &mut ScriptOutcome, root: NodeId, now: Instant) {
        let mut invalidation = std::mem::take(&mut outcome.invalidation);
        // Admission already refreshed these exact styles and their resources.
        // Retain paint work, not another selector pass; later author/resource
        // invalidations still merge their own STYLE/layout impacts normally.
        invalidation.impact = InvalidationImpact::PAINT;
        self.invalidation.merge_conservatively(invalidation, root);
        // A later hover cannot move an already promised rendering opportunity.
        self.deadline.get_or_insert(now + COLOR_PAINT_INTERVAL);
        outcome.render_requested = false;
    }

    pub(super) fn timer_micros(&self) -> Option<u64> {
        // Like JavaScript timers, the wire delay is relative to the last
        // logical clock advance, not this report. The browser subtracts elapsed
        // wall time once; input/resource reports must not reset the deadline.
        self.deadline
            .map(|deadline| micros(deadline.saturating_duration_since(self.clock_anchor)))
    }

    pub(super) fn merge_due(&mut self, outcome: &mut ScriptOutcome, root: NodeId, now: Instant) {
        if self.deadline.is_none_or(|deadline| now < deadline) {
            return;
        }
        outcome
            .invalidation
            .merge_conservatively(std::mem::take(&mut self.invalidation), root);
        outcome.request_full_render();
        self.deadline = None;
    }

    pub(super) fn clear(&mut self) {
        self.invalidation = RenderInvalidation::default();
        self.deadline = None;
    }
}

pub(super) fn color_only_change(
    invalidation: &RenderInvalidation,
    style: &StyleRefreshStats,
) -> bool {
    !invalidation.is_empty()
        && invalidation.impact == InvalidationImpact::STYLE
        && !invalidation.rebuild_style_rules
        && invalidation.removed_nodes.is_empty()
        && style.changed_styles > 0
        && style.removed_styles == 0
        && !style.layout_changed
        && !style.non_deferable_paint_changes
        && !style.full_rebuild
}

impl DocumentRuntime {
    pub(super) fn defer_input_color_paint(
        &mut self,
        outcome: &mut ScriptOutcome,
        style: &StyleRefreshStats,
        force_accessibility_update: bool,
    ) {
        if outcome.render_requested
            && !force_accessibility_update
            && !self.rendering_is_blocked()
            && !self
                .script_runtime
                .as_ref()
                .is_some_and(crate::engine::ScriptRuntime::has_pending_frame_layout)
            && color_only_change(&outcome.invalidation, style)
        {
            // Computed styles and DOM state are already current. The classifier
            // guarantees unchanged geometry and hit eligibility.
            // Retain ONLY visual work; report navigation, console, storage and
            // wheel decisions from this input immediately and exactly once.
            self.color_paint
                .defer(outcome, self.page.dom.document.id(), Instant::now());
        }
    }
}

#[cfg(test)]
mod tests;
