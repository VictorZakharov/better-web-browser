//! Bounded lossless delivery for browser-owned document-clock progress.

use crate::renderer_protocol::DocumentId;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Advance {
    pub(super) document: DocumentId,
    pub(super) elapsed: Duration,
    pub(super) max_callbacks: u32,
}

#[derive(Default)]
struct State {
    pending: Option<Advance>,
    receiver_open: bool,
}

pub(super) struct Sender {
    state: Arc<Mutex<State>>,
}

pub(super) struct Receiver {
    state: Arc<Mutex<State>>,
}

pub(super) fn bounded() -> (Sender, Receiver) {
    let state = Arc::new(Mutex::new(State {
        pending: None,
        receiver_open: true,
    }));
    (
        Sender {
            state: Arc::clone(&state),
        },
        Receiver { state },
    )
}

impl Sender {
    pub(super) fn send(&self, advance: Advance) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.receiver_open {
            return Err("renderer broker has exited".into());
        }
        state.pending = Some(match state.pending {
            Some(pending) if pending.document == advance.document => Advance {
                document: advance.document,
                elapsed: pending.elapsed.saturating_add(advance.elapsed),
                max_callbacks: pending.max_callbacks.saturating_add(advance.max_callbacks),
            },
            _ => advance,
        });
        Ok(())
    }
}

impl Receiver {
    pub(super) fn take(&self) -> Option<Advance> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pending
            .take()
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.receiver_open = false;
        state.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_document_progress_accumulates_in_one_slot() {
        let (sender, receiver) = bounded();
        let document = DocumentId::new(1).unwrap();
        sender
            .send(advance(document, Duration::from_millis(4), 2))
            .unwrap();
        sender
            .send(advance(document, Duration::from_millis(7), 3))
            .unwrap();
        assert_eq!(
            receiver.take(),
            Some(advance(document, Duration::from_millis(11), 5))
        );
        assert_eq!(receiver.take(), None);
    }

    #[test]
    fn replacement_document_supersedes_stale_progress() {
        let (sender, receiver) = bounded();
        sender
            .send(advance(
                DocumentId::new(1).unwrap(),
                Duration::from_secs(1),
                1,
            ))
            .unwrap();
        let replacement = advance(DocumentId::new(2).unwrap(), Duration::from_secs(2), 2);
        sender.send(replacement).unwrap();
        assert_eq!(receiver.take(), Some(replacement));
    }

    #[test]
    fn closing_receiver_rejects_later_progress() {
        let (sender, receiver) = bounded();
        drop(receiver);
        assert_eq!(
            sender.send(advance(DocumentId::new(1).unwrap(), Duration::ZERO, 1)),
            Err("renderer broker has exited".into())
        );
    }

    fn advance(document: DocumentId, elapsed: Duration, max_callbacks: u32) -> Advance {
        Advance {
            document,
            elapsed,
            max_callbacks,
        }
    }
}
