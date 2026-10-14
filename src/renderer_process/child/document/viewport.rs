//! Viewport changes and display-only cadence updates for a retained document.
use super::*;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn resize(
        &mut self,
        viewport: crate::renderer_protocol::PresentedViewport,
        connection: &mut ChildConnection,
    ) -> Result<AdvanceResult, String> {
        let viewport = viewport.validate().map_err(|error| error.to_string())?;
        let cadence_only = viewport.refresh_rate != self.viewport.refresh_rate
            && crate::renderer_protocol::PresentedViewport {
                refresh_rate: viewport.refresh_rate,
                ..self.viewport
            } == viewport;
        self.viewport = viewport;
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.set_rendering_environment(
                viewport.refresh_rate,
                self.lifecycle == crate::renderer_protocol::DocumentLifecycle::Active,
            );
        }
        // A monitor move can change cadence without changing CSS geometry. It
        // must not dispatch resize or reset the game's drawing buffer.
        if cadence_only {
            return Ok(AdvanceResult::Runtime(Box::new(RendererRuntimeUpdate {
                document: self.id,
                clock_advanced: false,
                runtime: runtime_report(
                    ScriptOutcome::default(),
                    self.script_runtime.is_some(),
                    self.media_runtime_report(),
                ),
                load: PageLoadReport::default(),
                next_timer_micros: self.next_timer_micros(),
            })));
        }
        self.apply_media_environment(viewport);
        self.text.borrow_mut().set_dpi(viewport.dpi);
        self.sync_script_layout_page();
        let mut outcome = self
            .dispatch_user_input(crate::engine::UserInputEvent::Viewport {
                width: viewport.style_width,
                height: viewport.height,
                layout_width: viewport.width,
                layout_height: viewport.height,
                scale: viewport.dpi as f32 / 96.0,
            })?
            .outcome;
        self.admit_user_input_outcome(&mut outcome, connection)?;
        let style = self
            .page
            .refresh_resources_for_viewport(viewport.style_width, viewport.height);
        self.start_presentational_preloads(connection)?;
        let started = Instant::now();
        self.rebuild_layout();
        let load = self.text.borrow_mut().finish_load_report(PageLoadReport {
            layout_micros: micros(started.elapsed()),
            ..PageLoadReport::default()
        });
        self.presentation(outcome, style, load, connection, None)
    }
}
