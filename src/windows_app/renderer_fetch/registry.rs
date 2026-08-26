//! Tab-scoped ownership of independently cancellable renderer Fetch requests.

use better_web_browser::fetch::{FetchController, FetchSignal};
use better_web_browser::renderer_protocol::DocumentId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(in crate::windows_app) struct RendererFetchRegistry {
    requests: Arc<Mutex<HashMap<(DocumentId, u64), FetchController>>>,
}

impl RendererFetchRegistry {
    pub(in crate::windows_app) fn register(
        &self,
        document: DocumentId,
        request_id: u64,
    ) -> FetchSignal {
        let controller = FetchController::new();
        let signal = controller.signal();
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert((document, request_id), controller);
        signal
    }

    pub(in crate::windows_app) fn abort(&self, document: DocumentId, request_id: u64) {
        if let Some(controller) = self
            .requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&(document, request_id))
        {
            controller.abort();
        }
    }

    pub(in crate::windows_app) fn complete(&self, document: DocumentId, request_id: u64) {
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&(document, request_id));
    }
}
