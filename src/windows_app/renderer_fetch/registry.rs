//! Tab-scoped ownership of independently cancellable renderer Fetch requests.

use better_web_browser::fetch::{FetchController, FetchSignal, RequestClient};
use better_web_browser::renderer_protocol::DocumentId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(in crate::windows_app) struct RendererFetchRegistry {
    pub(super) clients: Arc<Mutex<super::clients::Clients>>,
    requests: Arc<Mutex<HashMap<(DocumentId, u64), FetchController>>>,
}

impl RendererFetchRegistry {
    pub(in crate::windows_app) fn install_root(
        &self,
        document: DocumentId,
        url: &str,
        policy: Arc<better_web_browser::fetch::csp::PolicyContainer>,
    ) -> Result<(), String> {
        self.clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .install_root(document, url, policy)
            .map_err(|error| error.to_string())
    }
    pub(in crate::windows_app) fn resolve_client(
        &self,
        document: DocumentId,
        root: &str,
        requested: RequestClient,
    ) -> Result<super::clients::Client, String> {
        let mut clients = self
            .clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clients.activate(document);
        clients
            .resolve(document, root, requested)
            .map_err(|error| error.to_string())
    }
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
