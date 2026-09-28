//! PermissionStatus delivery into the owning document or child realm.

use super::*;
use crate::renderer_protocol::PermissionUpdate;

impl ScriptRuntime {
    pub fn deliver_permission_update(&mut self, update: PermissionUpdate) -> ScriptOutcome {
        let id = update.request_id as u32;
        if let Some(child) = self.child_for_fetch(id) {
            let owner = child.host.borrow().document.id();
            let outcome = child.deliver_permission_update(update);
            let outcome = self.collect_frame_result(owner, outcome);
            return self.finish_guarded_run(Ok(outcome));
        }
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
            if let Err(error) = super::super::host_call::permission::deliver_event(context, &update)
            {
                outcome
                    .errors
                    .push(format!("PermissionStatus event callback: {error}"));
            }
            super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript Permissions event", started.elapsed());
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
