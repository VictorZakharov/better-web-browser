//! Resolve selector-only input invalidations before deciding whether painting needs layout.
use super::*;
use crate::engine::invalidation::RenderInvalidation;

impl DocumentRuntime {
    pub(super) fn refresh_input_styles(
        &mut self,
        outcome: &mut ScriptOutcome,
    ) -> StyleRefreshStats {
        use crate::engine::invalidation::InvalidationImpact;
        if outcome.invalidation.impact == InvalidationImpact::STYLE {
            let stats = self
                .page
                .refresh_layout_styles_after_invalidation_for_viewport(
                    self.viewport.style_width,
                    self.viewport.height,
                    &outcome.invalidation,
                );
            if stats.changed_styles == 0 && stats.removed_styles == 0 && !stats.layout_changed {
                // :hover did not change any computed style or generated content. Keep the
                // retained layout and still return runtime effects/events to the browser.
                outcome.render_requested = false;
                return stats;
            }
            self.page.refresh_resources_after_invalidation_for_viewport(
                self.viewport.style_width,
                self.viewport.height,
                &RenderInvalidation::default(),
            );
            return stats;
        }
        self.page.refresh_resources_after_invalidation_for_viewport(
            self.viewport.style_width,
            self.viewport.height,
            &outcome.invalidation,
        )
    }
}
