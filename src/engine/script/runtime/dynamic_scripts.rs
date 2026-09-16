//! Readiness and completion API for document-owned script work.
use super::*;

impl ScriptRuntime {
    pub fn has_pending_dynamic_scripts(&self) -> bool {
        !self.host.borrow().pending_dynamic_scripts.is_empty()
    }

    pub fn pending_dynamic_script_requests(&self) -> Vec<DynamicScriptRequest> {
        self.host.borrow().pending_dynamic_scripts.requests()
    }

    pub fn take_dynamic_script_requests(&mut self) -> Vec<DynamicScriptRequest> {
        self.host
            .borrow_mut()
            .pending_dynamic_scripts
            .take_requests()
    }

    /// Network completion publishes readiness; it never executes JavaScript reentrantly.
    pub fn complete_dynamic_script(&mut self, node: NodeId, result: Result<String, String>) {
        if self.is_active() {
            self.host.borrow_mut().pending_dynamic_scripts.complete(
                node,
                result,
                self.total_script_bytes.get(),
            );
        }
    }

    pub fn has_runnable_dynamic_scripts(&self) -> bool {
        let host = self.host.borrow();
        host.pending_dynamic_scripts.has_ready()
            || host.pending_dynamic_scripts.has_unrequested()
            || host.module_jobs.dirty
            || !host.module_jobs.ready.is_empty()
    }

    pub fn has_ready_dynamic_scripts(&self) -> bool {
        let host = self.host.borrow();
        host.pending_dynamic_scripts.has_ready() || !host.module_jobs.ready.is_empty()
    }
}
