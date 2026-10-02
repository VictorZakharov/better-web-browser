//! Resolve stylesheet animations at document task checkpoints, including resource completion.
use super::*;

impl ScriptRuntime {
    pub(super) fn synchronize_css_animations(&mut self, outcome: &mut ScriptOutcome) {
        if outcome.runtime_stopped || !self.initialized {
            return;
        }
        let Some(context) = self.context.as_deref_mut() else {
            return;
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            context.call_global("__syncCssAnimations", &[])
        }));
        match result {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => outcome
                .errors
                .push(format!("resolve CSS animations: {error}")),
            Err(payload) => {
                self.context.take();
                self.frames.take();
                *outcome = stopped_runtime_outcome(panic_detail(payload));
            }
        }
    }
}
