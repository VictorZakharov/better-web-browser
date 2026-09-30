//! Resolve selector-only input invalidations before deciding whether painting needs layout.
use super::*;
use crate::engine::invalidation::RenderInvalidation;

impl DocumentRuntime {
    pub(in crate::renderer_process::child::document) fn presentation_after_user_input(
        &mut self,
        mut outcome: ScriptOutcome,
        input_kind: &'static str,
        force_accessibility_update: bool,
        wheel: Option<WheelAcknowledgement>,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        let needs_present = wheel.is_some() || force_accessibility_update
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
        let (style, style_time) = if outcome.render_requested {
            let started = Instant::now();
            let style = self.refresh_input_styles(&mut outcome);
            (style, started.elapsed())
        } else {
            (StyleRefreshStats::default(), Duration::ZERO)
        };
        self.defer_input_color_paint(&mut outcome, &style, force_accessibility_update);
        self.start_presentational_preloads(connection)?;
        let started = Instant::now();
        let rebuilt_layout = outcome.render_requested;
        if rebuilt_layout {
            self.rebuild_layout();
        }
        let load = self.text.borrow_mut().finish_load_report(PageLoadReport {
            style_micros: micros(style_time),
            layout_micros: micros(started.elapsed()),
            ..PageLoadReport::default()
        });
        if !outcome.render_requested && !force_accessibility_update {
            return Ok(Some(AdvanceResult::Runtime(Box::new(
                RendererRuntimeUpdate {
                    document: self.id,
                    clock_advanced: false,
                    next_timer_micros: self.next_timer_micros(),
                    runtime: runtime_report_with_wheel(
                        outcome,
                        self.script_runtime.is_some(),
                        self.media_runtime_report(),
                        wheel,
                    ),
                    load,
                },
            ))));
        }
        if !self.rendering_is_blocked() {
            // The full branch is already selected. Include its actual diagnostic bytes
            // before image admission; never create a report just to emit evidence.
            let diagnostic = (!self.diagnostic_selectors.is_empty()).then(|| {
                publication_diagnostics::Publication {
                    input_kind,
                    sequence: self.last_input_sequence,
                    mutations: outcome.mutation_count,
                    render_requested: outcome.render_requested,
                    forced_accessibility: force_accessibility_update,
                    rebuilt_layout,
                    sticky_layers: self.layout.sticky_offsets.len(),
                    displaced_sticky_layers: self
                        .layout
                        .sticky_offsets
                        .values()
                        .filter(|&&(x, y)| x != 0.0 || y != 0.0)
                        .count(),
                    layout_items: self.layout.items.len(),
                    style_time,
                    layout_time: Duration::from_micros(load.layout_micros),
                }
            });
            publication_diagnostics::append(&mut outcome.diagnostics, diagnostic);
        }
        self.presentation(outcome, style, load, connection, wheel)
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
            if stats.changed_styles == 0
                && stats.removed_styles == 0
                && !stats.layout_changed
                && !stats.non_deferable_paint_changes
            {
                // :hover did not change any computed style or generated content. Keep the
                // retained layout and still return runtime effects/events to the browser.
                outcome.render_requested = false;
                return stats;
            }
            if super::super::color_paint::color_only_change(&outcome.invalidation, &stats) {
                // Exact admitted colors/decorations cannot discover fonts, image
                // URLs or DOM resources. SVG currentColor rasterization DOES
                // depend on color, so refresh that narrow owning cache now.
                self.page.refresh_inline_svg_colors();
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
