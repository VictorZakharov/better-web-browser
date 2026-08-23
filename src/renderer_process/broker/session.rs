//! Document-scoped commands exposed by a renderer session.

use super::*;
use crate::renderer_protocol::{
    CookieStateSnapshot, DocumentStart, DocumentState, PresentedViewport,
};
use crate::storage::{StorageAreaKind, StorageAreaSnapshot};

impl RendererSession {
    pub(super) fn send_command(&self, command: worker::BrokerCommand) -> Result<(), String> {
        let result = self
            .commands
            .try_send(command)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "renderer command queue is full".to_string(),
                mpsc::TrySendError::Disconnected(_) => "renderer broker has exited".to_string(),
            });
        self.wake.notify();
        result
    }

    pub(super) fn send_blocking_command(
        &self,
        command: worker::BrokerCommand,
    ) -> Result<(), String> {
        self.wake.notify();
        let result = self
            .commands
            .send(command)
            .map_err(|_| "renderer broker has exited".to_string());
        self.wake.notify();
        result
    }

    pub fn load_document(
        &self,
        start: DocumentStart,
        state: DocumentState,
        body: Vec<u8>,
    ) -> Result<(), String> {
        self.send_command(worker::BrokerCommand::LoadDocument { start, state, body })
    }

    pub fn fetch_response_sink(&self, document: DocumentId) -> FetchResponseSink {
        FetchResponseSink::new(document, self.fetch_stream.clone(), self.wake.clone())
    }

    pub fn update_cookie_snapshot(&self, snapshot: CookieStateSnapshot) -> Result<(), String> {
        self.send_command(worker::BrokerCommand::UpdateCookieSnapshot(snapshot))
    }

    pub fn update_storage_snapshot(
        &self,
        document: DocumentId,
        area: StorageAreaKind,
        snapshot: StorageAreaSnapshot,
    ) -> Result<(), String> {
        self.send_command(worker::BrokerCommand::UpdateStorageSnapshot {
            document,
            area,
            snapshot,
        })
    }

    pub fn advance_time(
        &self,
        document: DocumentId,
        elapsed: Duration,
        max_callbacks: u32,
    ) -> Result<(), String> {
        self.send_command(worker::BrokerCommand::AdvanceTime {
            document,
            elapsed,
            max_callbacks,
        })
    }

    pub fn update_viewport(
        &self,
        document: DocumentId,
        viewport: PresentedViewport,
    ) -> Result<(), String> {
        self.send_command(worker::BrokerCommand::ViewportChanged { document, viewport })
    }

    pub fn cancel_document(&self, document: DocumentId) -> Result<(), String> {
        self.send_command(worker::BrokerCommand::CancelDocument(document))
    }
}
