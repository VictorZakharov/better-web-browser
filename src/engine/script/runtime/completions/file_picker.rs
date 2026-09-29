//! Browser-selected file snapshot delivery into the owning JavaScript realm.

use super::*;
use crate::renderer_protocol::FilePickerSelection;

impl ScriptRuntime {
    pub fn deliver_file_picker_selection(
        &mut self,
        request_id: u64,
        selection: FilePickerSelection,
    ) -> ScriptOutcome {
        let id = request_id as u32;
        if let Some(child) = self.child_for_fetch(id) {
            let owner = child.host.borrow().document.id();
            let outcome = child.deliver_file_picker_selection(request_id, selection);
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
            if let Err(error) = super::super::super::host_call::file_picker::deliver_event(
                context, request_id, selection,
            ) {
                outcome
                    .errors
                    .push(format!("File picker callback: {error}"));
            }
            super::super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript file picker callback", started.elapsed());
            outcome
        }));
        self.host.borrow().fetch_identifiers.borrow_mut().finish(id);
        self.finish_guarded_run(result)
    }
}
