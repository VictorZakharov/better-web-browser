//! An admitted resource retains its load obligation until success, failure, or cancellation.
use super::*;

impl DocumentRuntime {
    pub(super) fn register_document_load_resources(&mut self) {
        if self.pending_resource_preloads.is_empty()
            || self
                .script_runtime
                .as_ref()
                .is_some_and(|runtime| runtime.document_load_finished())
        {
            return;
        }
        let blockers = self.page.document_load_resources();
        for pending in &mut self.pending_resource_preloads {
            pending.load_blockers.extend(
                pending
                    .by_request
                    .values()
                    .filter(|resource| blockers.contains(*resource))
                    .cloned(),
            );
        }
    }

    pub(in crate::renderer_process::child::document) fn publish_document_load_readiness(
        &mut self,
        discovery_pending: bool,
    ) {
        let Some(runtime) = self.script_runtime.as_mut() else {
            return;
        };
        if runtime.document_load_finished() {
            return;
        }
        // fetch()/XHR, workers, and MSE traffic are intentionally not document-load blockers.
        // Removing a node does not erase an already admitted resource obligation. Completed
        // requests leave by_request, including unsuccessful responses and obsolete owners.
        let pending = discovery_pending
            || self.async_scripts.is_pending()
            || self.pending_resource_preloads.iter().any(|pending| {
                pending
                    .by_request
                    .values()
                    .any(|resource| pending.load_blockers.contains(resource))
            });
        runtime.set_document_load_pending(pending);
    }
}
