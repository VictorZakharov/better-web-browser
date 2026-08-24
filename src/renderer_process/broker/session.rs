//! Document-scoped commands exposed by a renderer session.

use super::*;
use crate::renderer_protocol::{
    CookieStateSnapshot, DocumentInput, DocumentStart, DocumentState, PresentationAcknowledgement,
    PresentedViewport,
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

    pub fn send_input(&self, input: DocumentInput) -> Result<(), String> {
        let coalescible = input.coalescible();
        match self.try_send_input(input)? {
            true => Ok(()),
            false if coalescible => Ok(()),
            false => Err("renderer command queue is full".into()),
        }
    }

    /// Attempts one validated input enqueue without treating backpressure as a renderer failure.
    pub fn try_send_input(&self, input: DocumentInput) -> Result<bool, String> {
        self.try_send_input_retained(input)
            .map(|pending| pending.is_none())
    }

    /// Returns the original validated input when the bounded broker channel is temporarily full.
    /// Browser UI callers use this to retain exact discrete-event ordering without blocking.
    pub fn try_send_input_retained(
        &self,
        input: DocumentInput,
    ) -> Result<Option<DocumentInput>, String> {
        input.validate().map_err(|error| error.to_string())?;
        let result = match self.commands.try_send(worker::BrokerCommand::Input(input)) {
            Ok(()) => Ok(None),
            Err(mpsc::TrySendError::Full(worker::BrokerCommand::Input(input))) => Ok(Some(input)),
            Err(mpsc::TrySendError::Full(_)) => unreachable!("only input commands are sent here"),
            Err(mpsc::TrySendError::Disconnected(_)) => Err("renderer broker has exited".into()),
        };
        self.wake.notify();
        result
    }

    pub fn acknowledge_presentation(
        &self,
        acknowledgement: PresentationAcknowledgement,
    ) -> Result<(), String> {
        acknowledgement
            .validate()
            .map_err(|error| error.to_string())?;
        self.send_command(worker::BrokerCommand::PresentationAcknowledged(
            acknowledgement,
        ))
    }

    pub fn cancel_document(&self, document: DocumentId) -> Result<(), String> {
        self.send_command(worker::BrokerCommand::CancelDocument(document))
    }
}
