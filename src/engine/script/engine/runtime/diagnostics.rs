//! Read-only engine diagnostics under the same isolate ownership as execution.
use super::*;

impl Context {
    pub(in crate::engine::script) fn take_cpu_task_diagnostics(&mut self) -> Vec<String> {
        self.agent.borrow_mut().take_cpu_diagnostics()
    }
    pub(in crate::engine::script) fn set_document_task_profiling(&mut self, enabled: bool) {
        self.agent.borrow_mut().set_task_profiling(enabled);
    }

    pub(in crate::engine::script) fn take_document_task_diagnostics(&mut self) -> Vec<String> {
        self.agent.borrow_mut().take_task_diagnostics()
    }

    pub(in crate::engine::script) fn set_execution_profiling(&mut self, enabled: bool) {
        self.agent.borrow_mut().set_gc_profiling(enabled);
    }

    pub(in crate::engine::script) fn gc_sample(&self) -> (u64, std::time::Duration) {
        self.agent.borrow().gc_sample()
    }

    pub(in crate::engine::script) fn heap_diagnostic(&mut self) -> JsResult<String> {
        self.agent.borrow_mut().run(|isolate| {
            let stats = isolate.get_heap_statistics();
            Ok(format!(
                "V8 heap: used={} committed={} physical={} external={} malloced={} contexts={} detached={}",
                stats.used_heap_size(),
                stats.total_heap_size(),
                stats.total_physical_size(),
                stats.external_memory(),
                stats.malloced_memory(),
                stats.number_of_native_contexts(),
                stats.number_of_detached_contexts(),
            ))
        })
    }
}
