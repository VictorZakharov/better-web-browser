//! Coalesced browser-authoritative cookie and Web Storage corrections.
//!
//! Page-generated command bursts must not make durable browser state look like a renderer
//! failure. Corrections therefore use three newest-wins slots instead of competing for the
//! ordinary bounded input/viewport command channel.

use crate::renderer_protocol::{CookieStateSnapshot, DocumentId};
use crate::storage::{StorageAreaKind, StorageAreaSnapshot};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub(super) enum StateUpdate {
    Cookie(CookieStateSnapshot),
    Storage {
        document: DocumentId,
        area: StorageAreaKind,
        snapshot: StorageAreaSnapshot,
    },
}

impl StateUpdate {
    pub(super) fn document(&self) -> DocumentId {
        match self {
            Self::Cookie(snapshot) => snapshot.document,
            Self::Storage { document, .. } => *document,
        }
    }

    fn same_slot(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Cookie(left), Self::Cookie(right)) => left.document == right.document,
            (
                Self::Storage {
                    document: left_document,
                    area: left_area,
                    ..
                },
                Self::Storage {
                    document: right_document,
                    area: right_area,
                    ..
                },
            ) => left_document == right_document && left_area == right_area,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) pending: usize,
    pub(super) submitted: u64,
    pub(super) coalesced: u64,
}

#[derive(Default)]
struct State {
    pending: VecDeque<StateUpdate>,
    in_flight: bool,
    submitted: u64,
    coalesced: u64,
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
        receiver_open: true,
        ..State::default()
    }));
    (
        Sender {
            state: Arc::clone(&state),
        },
        Receiver { state },
    )
}

impl Sender {
    pub(super) fn send_cookie(&self, snapshot: CookieStateSnapshot) -> Result<(), String> {
        snapshot.validate().map_err(|error| error.to_string())?;
        self.send(StateUpdate::Cookie(snapshot))
    }

    pub(super) fn send_storage(
        &self,
        document: DocumentId,
        area: StorageAreaKind,
        snapshot: StorageAreaSnapshot,
    ) -> Result<(), String> {
        snapshot.validate().map_err(|error| error.to_string())?;
        self.send(StateUpdate::Storage {
            document,
            area,
            snapshot,
        })
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        let state = lock(&self.state);
        Snapshot {
            pending: state.pending.len() + usize::from(state.in_flight),
            submitted: state.submitted,
            coalesced: state.coalesced,
        }
    }

    fn send(&self, update: StateUpdate) -> Result<(), String> {
        let mut state = lock(&self.state);
        if !state.receiver_open {
            return Err("renderer broker has exited".into());
        }
        state.submitted = state.submitted.saturating_add(1);
        let document = update.document();
        state
            .pending
            .retain(|pending| pending.document() == document);
        if let Some(pending) = state
            .pending
            .iter_mut()
            .find(|pending| pending.same_slot(&update))
        {
            *pending = update;
            state.coalesced = state.coalesced.saturating_add(1);
        } else {
            // One cookie plus two storage-area slots per document is the hard bound.
            state.pending.push_back(update);
        }
        Ok(())
    }
}

impl Receiver {
    pub(super) fn pending(&self) -> usize {
        let state = lock(&self.state);
        state.pending.len() + usize::from(state.in_flight)
    }

    pub(super) fn take(&self) -> Option<StateUpdate> {
        let mut state = lock(&self.state);
        if state.in_flight {
            return None;
        }
        let update = state.pending.pop_front();
        state.in_flight = update.is_some();
        update
    }

    pub(super) fn complete(&self) {
        lock(&self.state).in_flight = false;
    }

    pub(super) fn has_pending(&self) -> bool {
        let state = lock(&self.state);
        state.in_flight || !state.pending.is_empty()
    }

    pub(super) fn discard_document(&self, document: DocumentId) {
        lock(&self.state)
            .pending
            .retain(|update| update.document() != document);
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        let mut state = lock(&self.state);
        state.receiver_open = false;
        state.pending.clear();
        state.in_flight = false;
    }
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageEntry;

    #[test]
    fn repeated_updates_keep_only_the_latest_value_per_state_slot() {
        let (sender, receiver) = bounded();
        let document = DocumentId::new(1).unwrap();
        sender
            .send_storage(document, StorageAreaKind::Local, snapshot(2, "old"))
            .unwrap();
        sender
            .send_cookie(CookieStateSnapshot {
                document,
                version: 2,
                header: "old=1".into(),
            })
            .unwrap();
        sender
            .send_storage(document, StorageAreaKind::Local, snapshot(3, "latest"))
            .unwrap();
        sender
            .send_cookie(CookieStateSnapshot {
                document,
                version: 3,
                header: "latest=1".into(),
            })
            .unwrap();

        assert_eq!(
            sender.snapshot(),
            Snapshot {
                pending: 2,
                submitted: 4,
                coalesced: 2,
            }
        );
        match receiver.take().unwrap() {
            StateUpdate::Storage { snapshot, .. } => assert_eq!(snapshot.version, 3),
            update => panic!("unexpected first update: {update:?}"),
        }
        receiver.complete();
        match receiver.take().unwrap() {
            StateUpdate::Cookie(snapshot) => assert_eq!(snapshot.version, 3),
            update => panic!("unexpected second update: {update:?}"),
        }
        receiver.complete();
        assert!(receiver.take().is_none());
    }

    #[test]
    fn cancellation_discards_only_the_replaced_document() {
        let (sender, receiver) = bounded();
        let old = DocumentId::new(1).unwrap();
        let current = DocumentId::new(2).unwrap();
        sender
            .send_storage(old, StorageAreaKind::Local, snapshot(2, "old"))
            .unwrap();
        sender
            .send_storage(current, StorageAreaKind::Local, snapshot(2, "current"))
            .unwrap();
        receiver.discard_document(old);
        assert_eq!(receiver.take().unwrap().document(), current);
        receiver.complete();
        assert!(receiver.take().is_none());
    }

    fn snapshot(version: u64, value: &str) -> StorageAreaSnapshot {
        StorageAreaSnapshot {
            version,
            entries: vec![StorageEntry {
                key: "key".into(),
                value: value.into(),
            }],
        }
    }
}
