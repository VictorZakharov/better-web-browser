//! Notify the private FontFaceSet only at genuine environment transitions.
//! No polling timer is queued while a stylesheet or renderer layout is pending.
use super::*;

impl ScriptRuntime {
    pub(crate) fn set_font_stylesheets_pending(&mut self, pending: bool) {
        self.host.borrow_mut().font_environment.stylesheet_pending = pending;
    }

    pub(crate) fn set_pending_css_fonts(
        &mut self,
        sources: HashSet<crate::engine::font::loading_identity::FontLoadIdentity>,
    ) {
        let mut host = self.host.borrow_mut();
        if host.font_environment.pending_css_fonts != sources {
            // A transport can finish before the next JS checkpoint. Remember
            // admission for live faces so a fast success/error cannot erase
            // their loading period and loaded promise settlement. Retain only
            // the bounded CSS-connected face list, not every historic URL.
            let live = host
                .connected_font_faces()
                .into_iter()
                .map(|face| face.loading_identity())
                .collect::<HashSet<_>>();
            host.font_environment
                .requested_css_fonts
                .retain(|source| live.contains(source));
            host.font_environment.requested_css_fonts.extend(
                sources
                    .iter()
                    .filter(|source| live.contains(*source))
                    .cloned(),
            );
            host.font_environment.pending_css_fonts = sources;
            // CSS-connected face status can change while another face still
            // keeps the aggregate environment pending. Notify once per change.
            host.font_environment.notified_pending = None;
        }
    }

    pub(super) fn synchronize_font_environment(&mut self, outcome: &mut ScriptOutcome) {
        if outcome.runtime_stopped || !self.initialized || self.context.is_none() {
            return;
        }
        let pending = {
            let mut host = self.host.borrow_mut();
            if !host.font_environment_needs_notification() {
                return;
            }
            let pending = host.font_environment_pending();
            host.font_environment.notified_pending = Some(pending);
            pending
        };
        let context = self.context.as_deref_mut().expect("active document realm");
        let result = catch_unwind(AssertUnwindSafe(|| {
            context.call_private_hook("__fontEnvironmentChanged", &[JsValue::Boolean(pending)])
        }));
        match result {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => outcome
                .errors
                .push(format!("settle font environment: {error}")),
            Err(payload) => {
                self.context.take();
                self.frames.take();
                *outcome = stopped_runtime_outcome(panic_detail(payload));
            }
        }
    }
}
