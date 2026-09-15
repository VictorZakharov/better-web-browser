//! Fair event turns with byte/count-bounded adjacent storage transactions.
use super::notifications::EVENTS_PER_TURN;
use better_web_browser::renderer_process::RendererEvent;
use better_web_browser::renderer_protocol::StorageMutationRequest;

pub(super) fn storage_transaction(
    first: StorageMutationRequest,
    events: &mut std::iter::Peekable<impl Iterator<Item = RendererEvent>>,
) -> Vec<StorageMutationRequest> {
    let document = first.document;
    let area = first.mutation.area;
    let mut bytes = first.mutation.byte_len() + first.source_url.len();
    let mut requests = vec![first];
    // Old values add at most the initial 5 MiB map plus preceding writes.
    // Keep the transaction below an empty recipient queue's 32 MiB capacity.
    while matches!(events.peek(), Some(RendererEvent::StorageMutation(next))
        if next.document == document && next.mutation.area == area
            && bytes + next.mutation.byte_len() + next.source_url.len() <= 8 * 1024 * 1024)
    {
        if let Some(RendererEvent::StorageMutation(next)) = events.next() {
            bytes += next.mutation.byte_len() + next.source_url.len();
            requests.push(next);
        }
    }
    requests
}

const STORAGE_BATCH_ITEMS: usize = 256;
const STORAGE_BATCH_BYTES: usize = 1024 * 1024;

pub(super) fn collect(
    mut take: impl FnMut(&dyn Fn(&RendererEvent) -> bool) -> Option<RendererEvent>,
) -> Vec<RendererEvent> {
    let mut events = Vec::new();
    for _ in 0..EVENTS_PER_TURN {
        match take(&|_| true) {
            Some(event) => events.push(event),
            None => return events,
        }
    }
    let Some(RendererEvent::StorageMutation(last)) = events.last() else {
        return events;
    };
    let document = last.document;
    let area = last.mutation.area;
    let same_area = |event: &RendererEvent| {
        matches!(event,
        RendererEvent::StorageMutation(request)
        if request.document == document && request.mutation.area == area)
    };
    let (mut count, mut bytes) = events
        .iter()
        .rev()
        .take_while(|event| same_area(event))
        .fold((0, 0), |(count, bytes), event| {
            let RendererEvent::StorageMutation(request) = event else {
                unreachable!()
            };
            (count + 1, bytes + request.mutation.byte_len())
        });
    // Applying a small map mutation is cheap, but persisting the entire profile
    // for every 32 entries is not. Extend only an already-adjacent transaction,
    // never wait for more events, and retain both item and byte fairness bounds.
    while count < STORAGE_BATCH_ITEMS && bytes < STORAGE_BATCH_BYTES {
        let accepts = |event: &RendererEvent| {
            same_area(event)
                && matches!(event,
            RendererEvent::StorageMutation(request)
            if request.mutation.byte_len() <= STORAGE_BATCH_BYTES - bytes)
        };
        let Some(RendererEvent::StorageMutation(request)) = take(&accepts) else {
            break;
        };
        bytes += request.mutation.byte_len();
        count += 1;
        events.push(RendererEvent::StorageMutation(request));
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::renderer_protocol::{DocumentId, StorageMutationRequest};
    use better_web_browser::storage::{StorageAreaKind, StorageMutation, StorageOperation};
    use std::collections::VecDeque;

    fn storage(document: u64, area: StorageAreaKind, bytes: usize) -> RendererEvent {
        RendererEvent::StorageMutation(StorageMutationRequest {
            sequence: 1,
            source_url: "https://example.com/".into(),
            document: DocumentId::new(document).unwrap(),
            mutation: StorageMutation {
                area,
                expected_version: 1,
                operation: StorageOperation::Set {
                    key: "".into(),
                    value: "x".repeat(bytes).into(),
                },
            },
        })
    }
    fn diagnostic() -> RendererEvent {
        RendererEvent::Diagnostic {
            code: 1,
            text: "barrier".into(),
        }
    }
    fn drain(queue: &mut VecDeque<RendererEvent>) -> Vec<RendererEvent> {
        collect(|accepts| {
            if queue.front().is_some_and(accepts) {
                queue.pop_front()
            } else {
                None
            }
        })
    }
    #[test]
    fn non_storage_turns_keep_the_existing_fairness_budget() {
        let mut queue = (0..100).map(|_| diagnostic()).collect();
        assert_eq!(drain(&mut queue).len(), EVENTS_PER_TURN);
        assert_eq!(queue.len(), 100 - EVENTS_PER_TURN);
    }
    #[test]
    fn storage_extensions_are_bounded_by_both_count_and_bytes() {
        for (bytes, expected) in [(0, STORAGE_BATCH_ITEMS), (16 * 1024, 64)] {
            let mut queue = (0..300)
                .map(|_| storage(1, StorageAreaKind::Local, bytes))
                .collect();
            assert_eq!(drain(&mut queue).len(), expected);
            assert_eq!(queue.len(), 300 - expected);
        }
    }
    #[test]
    fn extension_does_not_consume_document_area_or_other_event_barriers() {
        for barrier in [
            diagnostic(),
            storage(2, StorageAreaKind::Local, 0),
            storage(1, StorageAreaKind::Session, 0),
        ] {
            let mut queue: VecDeque<_> = (0..40)
                .map(|_| storage(1, StorageAreaKind::Local, 0))
                .collect();
            queue.push_back(barrier);
            queue.push_back(storage(1, StorageAreaKind::Local, 0));
            assert_eq!(drain(&mut queue).len(), 40);
            assert_eq!(queue.len(), 2);
        }
    }
}
