use super::*;
use crate::renderer_protocol::{
    CookieStateSnapshot, DocumentId, ScrollInput, StorageSnapshotEnd, StorageSnapshotEntry,
    StorageSnapshotStart,
};
use crate::storage::{StorageAreaKind, StorageEntry};

#[test]
fn consecutive_scroll_updates_coalesce_while_fetching() {
    let document = DocumentId::new(1).unwrap();
    let message = |sequence, y| {
        BrowserMessage::Input(DocumentInput::Scroll(ScrollInput {
            document,
            sequence,
            x: 0.0,
            y,
        }))
    };
    assert!(can_replace_deferred(&message(1, 10.0), &message(2, 20.0)));
}

#[test]
fn ordered_input_is_never_coalesced() {
    let document = DocumentId::new(1).unwrap();
    let scroll = BrowserMessage::Input(DocumentInput::Scroll(ScrollInput {
        document,
        sequence: 1,
        x: 0.0,
        y: 10.0,
    }));
    let lifecycle = BrowserMessage::Input(DocumentInput::Lifecycle(
        crate::renderer_protocol::LifecycleInput {
            document,
            sequence: 2,
            state: crate::renderer_protocol::DocumentLifecycle::Hidden,
        },
    ));
    assert!(!can_replace_deferred(&scroll, &lifecycle));
}

#[test]
fn one_wire_batch_splits_into_independent_completion_sets() {
    let document = DocumentId::new(1).unwrap();
    let pending = PendingFetchBatch {
        document,
        batch_id: 3,
        expected: HashSet::from([10, 11, 12]),
    };
    let (first, deferred) = pending.split(HashSet::from([10, 12])).unwrap();
    assert_eq!(first.unwrap().expected, HashSet::from([10, 12]));
    assert_eq!(deferred.unwrap().expected, HashSet::from([11]));
}

#[test]
fn authoritative_state_transfers_use_reserved_deferred_capacity() {
    let document = DocumentId::new(1).unwrap();
    let messages = [
        BrowserMessage::CookieSnapshot(CookieStateSnapshot {
            document,
            version: 1,
            header: String::new(),
        }),
        BrowserMessage::StorageSnapshotStart(StorageSnapshotStart {
            document,
            area: StorageAreaKind::Local,
            version: 1,
            entry_count: 1,
        }),
        BrowserMessage::StorageSnapshotEntry(StorageSnapshotEntry {
            document,
            area: StorageAreaKind::Local,
            entry: StorageEntry {
                key: "key".into(),
                value: "value".into(),
            },
        }),
        BrowserMessage::StorageSnapshotEnd(StorageSnapshotEnd {
            document,
            area: StorageAreaKind::Local,
            version: 1,
        }),
    ];
    assert!(messages.iter().all(is_state_transfer));
}

#[test]
fn complete_maximum_storage_snapshot_survives_full_ordinary_mailbox() {
    let document = DocumentId::new(1).unwrap();
    let mut pending = VecDeque::new();
    for sequence in 1..=MAX_DEFERRED_RENDERER_MESSAGES as u64 {
        pending.push_back(BrowserMessage::Input(DocumentInput::Lifecycle(
            crate::renderer_protocol::LifecycleInput {
                document,
                sequence,
                state: crate::renderer_protocol::DocumentLifecycle::Hidden,
            },
        )));
    }
    let start = BrowserMessage::StorageSnapshotStart(StorageSnapshotStart {
        document,
        area: StorageAreaKind::Local,
        version: 3,
        entry_count: crate::limits::MAX_STORAGE_ENTRIES_PER_ORIGIN as u32,
    });
    assert!(has_deferred_capacity(&pending, &start));
    pending.push_back(start);
    for index in 0..crate::limits::MAX_STORAGE_ENTRIES_PER_ORIGIN {
        let entry = BrowserMessage::StorageSnapshotEntry(StorageSnapshotEntry {
            document,
            area: StorageAreaKind::Local,
            entry: StorageEntry {
                key: index.to_string(),
                value: String::new(),
            },
        });
        assert!(has_deferred_capacity(&pending, &entry));
        pending.push_back(entry);
    }
    let end = BrowserMessage::StorageSnapshotEnd(StorageSnapshotEnd {
        document,
        area: StorageAreaKind::Local,
        version: 3,
    });
    assert!(has_deferred_capacity(&pending, &end));
    pending.push_back(end);
    assert_eq!(
        pending
            .iter()
            .filter(|message| is_state_transfer(message))
            .count(),
        MAX_DEFERRED_RENDERER_STATE_MESSAGES
    );
    assert!(!has_deferred_capacity(
        &pending,
        &BrowserMessage::CookieSnapshot(CookieStateSnapshot {
            document,
            version: 4,
            header: String::new(),
        })
    ));
}
