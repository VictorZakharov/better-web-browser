//! Bounded newest-wins delivery for browser presentation acknowledgements.

use crate::renderer_protocol::PresentationAcknowledgement;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    pending: Option<PresentationAcknowledgement>,
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
    pub(super) fn send(&self, acknowledgement: PresentationAcknowledgement) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.receiver_open {
            return Err("renderer broker has exited".into());
        }
        let replace = state.pending.is_none_or(|pending| {
            pending.document != acknowledgement.document
                || pending.revision <= acknowledgement.revision
        });
        if replace {
            state.pending = Some(acknowledgement);
        }
        Ok(())
    }
}

impl Receiver {
    pub(super) fn take(&self) -> Option<PresentationAcknowledgement> {
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
    use crate::renderer_protocol::DocumentId;

    #[test]
    fn pending_acknowledgements_keep_the_newest_document_revision() {
        let (sender, receiver) = bounded();
        let first = DocumentId::new(1).unwrap();
        let replacement = DocumentId::new(2).unwrap();
        sender.send(acknowledgement(first, 3)).unwrap();
        sender.send(acknowledgement(first, 2)).unwrap();
        sender.send(acknowledgement(first, 4)).unwrap();
        sender.send(acknowledgement(replacement, 1)).unwrap();
        assert_eq!(receiver.take(), Some(acknowledgement(replacement, 1)));
        assert_eq!(receiver.take(), None);
    }

    #[test]
    fn closing_the_receiver_rejects_later_acknowledgements() {
        let (sender, receiver) = bounded();
        drop(receiver);
        assert_eq!(
            sender.send(acknowledgement(DocumentId::new(1).unwrap(), 1)),
            Err("renderer broker has exited".into())
        );
    }

    fn acknowledgement(document: DocumentId, revision: u64) -> PresentationAcknowledgement {
        PresentationAcknowledgement {
            document,
            revision,
            presented: true,
            controls_applied: true,
        }
    }
}
