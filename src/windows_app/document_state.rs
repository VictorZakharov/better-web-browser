//! Browser-authoritative application of renderer cookie and Web Storage intents.

use super::*;
use better_web_browser::renderer_protocol::{
    CookieMutation, CookieStateSnapshot, StorageMutationRequest,
};
use better_web_browser::storage::StorageAreaKind;

impl BrowserState {
    pub(super) fn apply_renderer_cookie_mutation(
        &mut self,
        mutation: CookieMutation,
    ) -> Result<(), String> {
        if !self.navigation.owns_document(mutation.document) {
            return Ok(());
        }
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
        }
        Ok(())
    }

    pub(super) fn apply_renderer_storage_mutation(
        &mut self,
        request: StorageMutationRequest,
    ) -> Result<(), String> {
        if !self.navigation.owns_document(request.document) {
            return Ok(());
        }
        let document_url = self.reader_url.clone();
        let result = match request.mutation.area {
            StorageAreaKind::Local => self.local_storage.apply(&document_url, &request.mutation),
            StorageAreaKind::Session => {
                self.session_storage.apply(&document_url, &request.mutation)
            }
        };
        if let Err(error) = result {
            self.status_text = format!("Web Storage update failed: {error}");
        }
        let snapshot = match request.mutation.area {
            StorageAreaKind::Local => self.local_storage.snapshot(&document_url),
            StorageAreaKind::Session => self.session_storage.snapshot(&document_url),
        }
        .map_err(|error| format!("read Web Storage state: {error}"))?;
        if let Some(session) = self.renderer_session.as_ref() {
            session
                .update_storage_snapshot(request.document, request.mutation.area, snapshot)
                .map_err(|error| format!("synchronize Web Storage: {error}"))?;
        }
        Ok(())
    }
}
