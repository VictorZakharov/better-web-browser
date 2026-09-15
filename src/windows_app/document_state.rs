//! Browser-authoritative application of renderer cookie and Web Storage intents.

use super::*;
use better_web_browser::renderer_protocol::{
    CookieMutation, CookieStateSnapshot, StorageMutationRequest,
};

impl BrowserState {
    pub(super) fn apply_renderer_cookie_mutation(
        &mut self,
        mutation: CookieMutation,
    ) -> Result<(), String> {
        if !self.navigation.owns_document(mutation.document) {
            return Ok(());
        }
        self.incidents.cookie_mutations = self.incidents.cookie_mutations.saturating_add(1);
        self.incidents
            .record("cookie", "renderer mutation received");
        if let Err(error) = self
            .http_client
            .set_cookie(&self.reader_url, &mutation.assignment)
        {
            self.status_text = format!("document.cookie update failed: {error}");
        }
        let snapshot = self
            .http_client
            .document_cookie_snapshot(&self.reader_url)
            .map_err(|error| format!("read document.cookie state: {error}"))?;
        let correction = CookieStateSnapshot {
            document: mutation.document,
            version: snapshot.version,
            header: snapshot.header,
        };
        if let Some(session) = self.renderer_session.as_ref() {
            session
                .update_cookie_snapshot(correction)
                .map_err(|error| format!("synchronize document.cookie: {error}"))?;
            let telemetry = session.snapshot();
            self.incidents.record(
                "cookie",
                format!(
                    "authoritative state queued; pending={}, submitted={}, coalesced={}",
                    telemetry.pending_state_updates,
                    telemetry.submitted_state_updates,
                    telemetry.coalesced_state_updates
                ),
            );
        }
        Ok(())
    }

    pub(super) fn apply_renderer_storage_mutations(
        &mut self,
        requests: &[StorageMutationRequest],
    ) -> Result<bool, String> {
        let Some(request) = requests.first() else {
            return Ok(true);
        };
        if !self.navigation.owns_document(request.document) {
            return Ok(true);
        }
        let app = self.app.clone();
        let tab = self.tabs.active_mut();
        let Some((document, subscription)) = &tab.storage_subscription else {
            return Err("active document has no Web Storage subscription".into());
        };
        if *document != request.document {
            return Ok(true);
        }
        if requests.iter().any(|request| request.document != *document) {
            return Err("storage batch crosses document identity".into());
        }
        let writes = requests
            .iter()
            .map(|request| better_web_browser::storage::StorageWrite {
                sequence: request.sequence,
                source_url: request.source_url.clone(),
                mutation: request.mutation.clone(),
            })
            .collect::<Vec<_>>();
        let applied = app
            .storage_coordinator
            .apply(subscription, &writes, &mut tab.session_storage)
            .map_err(|error| format!("synchronize Web Storage: {error}"))?;
        if applied {
            if let Some(error) = subscription.take_error() {
                tab.status_text = format!("Web Storage update failed: {error}");
                tab.incidents.record("storage", &tab.status_text);
            }
            tab.incidents.storage_mutations = tab
                .incidents
                .storage_mutations
                .saturating_add(writes.len() as u64);
            tab.incidents.record(
                "storage",
                format!("{} ordered writes committed or acknowledged", writes.len()),
            );
        }
        Ok(applied)
    }

    pub(super) fn pump_storage_updates(&mut self, id: super::tabs::TabId) -> Result<(), String> {
        let Some(tab) = self.tabs.get_mut(id) else {
            return Ok(());
        };
        let Some((document, subscription)) = &tab.storage_subscription else {
            return Ok(());
        };
        if !tab.navigation.owns_document(*document) {
            tab.storage_subscription = None;
            return Ok(());
        }
        let Some(session) = &tab.renderer_session else {
            return Ok(());
        };
        let mut error = None;
        subscription.take_if(|update| {
            match session.try_synchronize_storage(*document, update.clone()) {
                Ok(accepted) => accepted,
                Err(detail) => {
                    error = Some(detail);
                    false
                }
            }
        });
        error.map_or(Ok(()), Err)
    }
}
