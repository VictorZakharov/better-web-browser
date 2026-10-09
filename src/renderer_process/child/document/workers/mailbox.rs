//! Bounded, nonblocking delivery from the document renderer to a dedicated Worker.

use super::*;
use crate::renderer_protocol::WebSocketEventKind;
use std::sync::atomic::AtomicUsize;

// A burst of progressive Fetches may deliver many small chunks before the
// Worker can run its next task. Leave room for that ordinary workload while
// retaining a finite backstop for an unresponsive Worker.
pub(super) const MAX_PENDING_COMMANDS: usize = 1024;
pub(super) const MAX_WORKER_PENDING_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_DOCUMENT_WORKER_PENDING_BYTES: usize = 128 * 1024 * 1024;
const COMMAND_OVERHEAD_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdmissionError {
    Saturated,
    Disconnected,
}

pub(super) struct Mailbox {
    sender: mpsc::SyncSender<QueuedCommand>,
    worker_bytes: Arc<AtomicUsize>,
    document_bytes: Arc<AtomicUsize>,
}

pub(super) struct QueuedCommand {
    command: Option<WorkerCommand>,
    charged_bytes: usize,
    worker_bytes: Arc<AtomicUsize>,
    document_bytes: Arc<AtomicUsize>,
}

impl QueuedCommand {
    pub(super) fn take(&mut self) -> WorkerCommand {
        self.command.take().expect("a Worker command is taken once")
    }
}

impl Drop for QueuedCommand {
    fn drop(&mut self) {
        self.worker_bytes
            .fetch_sub(self.charged_bytes, Ordering::AcqRel);
        self.document_bytes
            .fetch_sub(self.charged_bytes, Ordering::AcqRel);
    }
}

impl Mailbox {
    pub(super) fn new(document_bytes: Arc<AtomicUsize>) -> (Self, mpsc::Receiver<QueuedCommand>) {
        let (sender, receiver) = mpsc::sync_channel(MAX_PENDING_COMMANDS);
        (
            Self {
                sender,
                worker_bytes: Arc::new(AtomicUsize::new(0)),
                document_bytes,
            },
            receiver,
        )
    }

    pub(super) fn try_send(&self, command: WorkerCommand) -> Result<(), AdmissionError> {
        let bytes = command
            .payload_bytes()
            .saturating_add(COMMAND_OVERHEAD_BYTES);
        if !reserve(&self.worker_bytes, MAX_WORKER_PENDING_BYTES, bytes) {
            return Err(AdmissionError::Saturated);
        }
        if !reserve(
            &self.document_bytes,
            MAX_DOCUMENT_WORKER_PENDING_BYTES,
            bytes,
        ) {
            self.worker_bytes.fetch_sub(bytes, Ordering::AcqRel);
            return Err(AdmissionError::Saturated);
        }
        let queued = QueuedCommand {
            command: Some(command),
            charged_bytes: bytes,
            worker_bytes: self.worker_bytes.clone(),
            document_bytes: self.document_bytes.clone(),
        };
        match self.sender.try_send(queued) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => Err(AdmissionError::Saturated),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(AdmissionError::Disconnected),
        }
    }
}

fn reserve(counter: &AtomicUsize, limit: usize, bytes: usize) -> bool {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
            used.checked_add(bytes).filter(|total| *total <= limit)
        })
        .is_ok()
}

impl WorkerCommand {
    fn payload_bytes(&self) -> usize {
        match self {
            Self::Message(serialized) => serialized.capacity(),
            Self::PortMessage { serialized, .. } => serialized.capacity(),
            Self::Database { payload, .. } => payload.capacity(),
            Self::Fetch { event, .. } => match event {
                ScriptFetchEvent::Chunk(bytes) => bytes.capacity(),
                ScriptFetchEvent::Head(Ok(response)) => {
                    response.body.len()
                        + response
                            .url_list
                            .iter()
                            .map(|url| url.as_str().len().saturating_mul(3).saturating_add(128))
                            .sum::<usize>()
                        + response
                            .headers
                            .iter()
                            .map(|header| {
                                header.name().len() + header.value().len() + COMMAND_OVERHEAD_BYTES
                            })
                            .sum::<usize>()
                }
                ScriptFetchEvent::Head(Err(error)) | ScriptFetchEvent::Abort(error) => {
                    error.message().len()
                }
                ScriptFetchEvent::End => 0,
            },
            Self::WebSocket(event) => match &event.kind {
                WebSocketEventKind::Message { data, .. } => data.capacity(),
                WebSocketEventKind::Open { protocol } => protocol.capacity(),
                WebSocketEventKind::Close { reason, .. } => reason.capacity(),
                WebSocketEventKind::Sent { .. } | WebSocketEventKind::Error => 0,
            },
            Self::PortClose(_) | Self::Terminate => 0,
        }
    }
}

#[cfg(test)]
mod execution_tests;
#[cfg(test)]
mod tests;
