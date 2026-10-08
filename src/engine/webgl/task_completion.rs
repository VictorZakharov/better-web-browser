//! Trusted event-loop publication, separate from the author command protocol.
use super::{ApiVersion, BackendContexts, Result, WebGl};

impl WebGl {
    pub(super) fn complete_gpu_task(&mut self) -> Result<()> {
        if self.objects.poisoned {
            return Err(super::gl::OUT_OF_MEMORY);
        }
        self.progress_program_links(1)?;
        if self.options.api == ApiVersion::Two {
            // No finish, blocking wait, or invented completion. Cache only native
            // results that are ready after returning from a task and its jobs.
            // https://registry.khronos.org/webgl/specs/latest/2.0/#5.14.11
            self.publish_sync_results()?;
            self.publish_query_results()?;
        }
        Ok(())
    }
}

impl BackendContexts {
    pub(super) fn complete_task(&mut self, ids: &[u32]) -> Vec<u32> {
        let mut lost = Vec::new();
        for &id in ids {
            let Some(context) = self.contexts.get_mut(&id) else {
                lost.push(id);
                continue;
            };
            if context.options.api != ApiVersion::Two {
                continue;
            }
            if context.native.make_current().is_err() || context.complete_gpu_task().is_err() {
                lost.push(id);
            }
        }
        for &id in &lost {
            self.remove(id);
        }
        lost
    }
}
