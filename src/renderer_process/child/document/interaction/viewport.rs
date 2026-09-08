//! Native viewport events schedule the existing rendering-observer task after their microtasks.

use super::*;

impl DocumentRuntime {
    pub(in crate::renderer_process::child::document) fn dispatch_user_input(
        &mut self,
        event: UserInputEvent,
    ) -> Result<crate::engine::UserInputResult, String> {
        let Some(runtime) = self.script_runtime.as_mut() else {
            return Ok(crate::engine::UserInputResult {
                default_allowed: true,
                ..Default::default()
            });
        };
        let viewport_changed = matches!(
            &event,
            UserInputEvent::Scroll { .. } | UserInputEvent::Viewport { .. }
        );
        let result = runtime.dispatch_user_input(event);
        // Native dispatch drains Promise jobs before returning. Coalesce with any geometry
        // publication and deliver from advance(), not from cancelable author event propagation.
        // https://www.w3.org/TR/intersection-observer/#queue-intersection-observer-task
        if viewport_changed && !result.outcome.runtime_stopped {
            self.geometry_observers_pending = true;
        }
        Ok(result)
    }

    pub(in crate::renderer_process::child) fn pending_geometry_observer_update(
        &mut self,
    ) -> Option<RendererRuntimeUpdate> {
        if !self.geometry_observers_pending {
            return None;
        }
        Some(RendererRuntimeUpdate {
            document: self.id,
            clock_advanced: false,
            runtime: runtime_report(
                ScriptOutcome::default(),
                self.script_runtime.is_some(),
                self.media_runtime_report(),
            ),
            load: PageLoadReport::default(),
            next_timer_micros: self.next_timer_micros(),
        })
    }
}
