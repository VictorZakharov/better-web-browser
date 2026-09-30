//! Resolve selector-only input invalidations before deciding whether painting needs layout.
use super::*;
use crate::engine::invalidation::RenderInvalidation;

impl DocumentRuntime {
    pub(in crate::renderer_process::child::document) fn presentation_after_user_input(
        &mut self,
        mut outcome: ScriptOutcome,
        force_accessibility_update: bool,
        force_runtime_report: bool,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        let needs_present = force_runtime_report || force_accessibility_update
            || outcome.render_requested
            || outcome.executed > 0
            || !outcome.errors.is_empty()
            || !outcome.console.is_empty()
            || !outcome.diagnostics.is_empty()
            || outcome.navigation_url.is_some()
            // Quiet input can queue a form-navigation task. Geometry observers already
            // publish their own wakeup after sampling; do not bypass that checkpoint.
            || (!self.has_pending_geometry_observers() && self.next_timer_micros().is_some())
            || outcome.viewport_scroll_y.is_some()
            || outcome.viewport_wheel_delta_y != 0.0
            || !outcome.history_actions.is_empty()
            || !outcome.cookie_updates.is_empty();
        if !needs_present {
            return Ok(None);
        }
        let style = if outcome.render_requested {
            self.refresh_input_styles(&mut outcome)
        } else {
            StyleRefreshStats::default()
        };
        self.start_presentational_preloads(connection)?;
        let started = Instant::now();
        if outcome.render_requested {
            self.rebuild_layout();
        }
        let load = self.text.borrow_mut().finish_load_report(PageLoadReport {
            layout_micros: micros(started.elapsed()),
            ..PageLoadReport::default()
        });
        if !outcome.render_requested && !force_accessibility_update {
            return Ok(Some(AdvanceResult::Runtime(Box::new(
                RendererRuntimeUpdate {
                    document: self.id,
                    clock_advanced: false,
                    next_timer_micros: self.next_timer_micros(),
                    runtime: runtime_report(
                        outcome,
                        self.script_runtime.is_some(),
                        self.media_runtime_report(),
                    ),
                    load,
                },
            ))));
        }
        self.presentation(outcome, style, load, connection)
            .map(Some)
    }

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
