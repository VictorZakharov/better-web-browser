use super::*;
use crate::renderer_protocol::{DocumentId, StorageMutationRequest};
use crate::storage::{StorageAreaKind, StorageMutation, StorageOperation};

fn mutation(version: u64) -> RendererEvent {
    RendererEvent::StorageMutation(StorageMutationRequest {
        sequence: 1,
        source_url: "https://example.com/".into(),
        document: DocumentId::new(1).unwrap(),
        mutation: StorageMutation {
            area: StorageAreaKind::Local,
            expected_version: version,
            operation: StorageOperation::Set {
                key: "".into(),
                value: "x".repeat(4 * 1024 * 1024).into(),
            },
        },
    })
}

#[test]
fn storage_byte_backpressure_waits_for_space_and_preserves_order() {
    let (sender, receiver) = bounded();
    for version in 1..=8 {
        sender.send(mutation(version)).unwrap();
    }
    let (finished_tx, finished_rx) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        sender.send(mutation(9)).unwrap();
        finished_tx.send(()).unwrap();
    });
    assert!(finished_rx.recv_timeout(Duration::from_millis(50)).is_err());
    for version in 1..=9 {
        let RendererEvent::StorageMutation(request) =
            receiver.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("wrong event");
        };
        assert_eq!(request.mutation.expected_version, version);
    }
    finished_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    producer.join().unwrap();
}

#[test]
fn teardown_releases_a_storage_writer_waiting_for_space() {
    let (sender, receiver) = bounded();
    for version in 1..=8 {
        sender.send(mutation(version)).unwrap();
    }
    let producer = std::thread::spawn(move || sender.send(mutation(9)));
    drop(receiver);
    producer.join().unwrap().unwrap();
}

fn fill_storage_slots(sender: &EventSender) {
    for version in 1..=MAX_QUEUED_RENDERER_EVENTS {
        sender
            .send(RendererEvent::StorageMutation(StorageMutationRequest {
                sequence: 1,
                source_url: "https://example.com/".into(),
                document: DocumentId::new(1).unwrap(),
                mutation: StorageMutation {
                    area: StorageAreaKind::Local,
                    expected_version: version as u64,
                    operation: StorageOperation::Clear,
                },
            }))
            .unwrap();
    }
}

#[test]
fn trailing_event_waits_behind_a_full_storage_queue() {
    let (sender, receiver) = bounded();
    fill_storage_slots(&sender);
    let (finished_tx, finished_rx) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        sender
            .send(RendererEvent::Diagnostic {
                code: 0,
                text: "barrier".into(),
            })
            .unwrap();
        finished_tx.send(()).unwrap();
    });
    assert!(finished_rx.recv_timeout(Duration::from_millis(50)).is_err());
    for version in 1..=MAX_QUEUED_RENDERER_EVENTS {
        let RendererEvent::StorageMutation(request) =
            receiver.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("barrier overtook a storage mutation");
        };
        assert_eq!(request.mutation.expected_version, version as u64);
    }
    assert!(
        matches!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), RendererEvent::Diagnostic { text, .. } if text == "barrier")
    );
    finished_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    producer.join().unwrap();
}

#[test]
fn teardown_releases_the_event_after_a_full_storage_queue() {
    let (sender, receiver) = bounded();
    fill_storage_slots(&sender);
    let producer = std::thread::spawn(move || sender.send(RendererEvent::Unresponsive));
    drop(receiver);
    // The receiver may close before or after the producer obtains the lock.
    let _ = producer.join().unwrap();
}

#[test]
fn conditional_receive_never_skips_or_discards_a_front_barrier() {
    let (sender, receiver) = bounded();
    sender
        .send(RendererEvent::Diagnostic {
            code: 1,
            text: "barrier".into(),
        })
        .unwrap();
    sender.send(mutation(1)).unwrap();
    assert!(matches!(
        receiver.try_recv_if(|event| matches!(event, RendererEvent::StorageMutation(_))),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(receiver.pending(), 2);
    assert!(matches!(
        receiver.try_recv().unwrap(),
        RendererEvent::Diagnostic { .. }
    ));
    assert!(matches!(
        receiver
            .try_recv_if(|event| matches!(event, RendererEvent::StorageMutation(_)))
            .unwrap(),
        RendererEvent::StorageMutation(_)
    ));
}
