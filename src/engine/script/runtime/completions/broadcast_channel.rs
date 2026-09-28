//! Browser-to-document BroadcastChannel task delivery.

use super::*;
use crate::renderer_protocol::BroadcastDelivery;

impl ScriptRuntime {
    pub fn deliver_broadcast(&mut self, delivery: &BroadcastDelivery) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            host.borrow_mut().begin_task();
            let mut outcome = ScriptOutcome::default();
            let started = Instant::now();
            if let Err(error) = context.call_global(
                "__dispatchBroadcastChannel",
                &[
                    JsValue::from(delivery.channel_id as u32),
                    JsValue::from(JsString::from(delivery.serialized.as_str())),
                    JsValue::from(JsString::from(delivery.origin.as_str())),
                ],
            ) {
                outcome
                    .errors
                    .push(format!("BroadcastChannel callback: {error}"));
            }
            super::super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript BroadcastChannel callback", started.elapsed());
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
