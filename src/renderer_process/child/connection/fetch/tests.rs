use super::*;
use crate::renderer_protocol::{DocumentId, ScrollInput};

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
