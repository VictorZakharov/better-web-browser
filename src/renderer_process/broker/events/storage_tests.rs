use super::*;
use crate::renderer_protocol::{DocumentId, StorageMutationRequest};
use crate::storage::{StorageAreaKind, StorageMutation, StorageOperation};

fn mutation(version: u64) -> RendererEvent {
    RendererEvent::StorageMutation(StorageMutationRequest {
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
