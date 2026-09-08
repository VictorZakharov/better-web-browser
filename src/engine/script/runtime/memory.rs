//! Bounded memory attribution for opt-in runtime diagnostics.

use super::*;

const HEAP_SAMPLE_INTERVAL: Duration = Duration::from_secs(5);

impl ScriptRuntime {
    pub(super) fn append_memory_diagnostic(&mut self, outcome: &mut ScriptOutcome) {
        if !self.host.borrow().host_call_profile.is_enabled()
            || self
                .last_heap_sample
                .is_some_and(|sampled| sampled.elapsed() < HEAP_SAMPLE_INTERVAL)
        {
            return;
        }
        let Some(context) = self.context.as_deref_mut() else {
            return;
        };
        match context.heap_diagnostic() {
            Ok(diagnostic) => outcome.diagnostics.push(diagnostic),
            Err(error) => outcome
                .diagnostics
                .push(format!("could not sample the V8 heap: {error}")),
        }
        self.last_heap_sample = Some(Instant::now());
    }
}
