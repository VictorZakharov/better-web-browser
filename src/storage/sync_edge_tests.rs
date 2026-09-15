use super::*;

#[test]
fn long_local_journal_drains_to_the_authoritative_map() {
    let local = Arc::new(LocalStorage::in_memory());
    let hub = StorageCoordinator::new(Arc::clone(&local));
    let (_, source) = hub.subscribe(URL).unwrap();
    let mut projection = StorageProjection::default();
    let mut writes = Vec::new();
    for pass in 0..3 {
        if pass > 0 {
            writes.push(
                projection
                    .write(StorageAreaKind::Local, URL, StorageOperation::Clear)
                    .unwrap()
                    .unwrap(),
            );
        }
        for index in 0..1000 {
            writes.push(
                projection
                    .write(
                        StorageAreaKind::Local,
                        URL,
                        StorageOperation::Set {
                            key: format!("key-{index}").into(),
                            value: "x".repeat(4096).into(),
                        },
                    )
                    .unwrap()
                    .unwrap(),
            );
        }
    }
    let mut session = SessionStorage::default();
    for batch in writes.chunks(128) {
        assert!(hub.apply(&source, batch, &mut session).unwrap());
    }
    let updates = drain(&source, &mut projection);
    assert_eq!(updates.len(), writes.len());
    assert_eq!(projection.view().snapshot(), local.snapshot(URL).unwrap());
}

#[test]
fn concurrent_quota_rejection_retires_only_the_rejected_intent_without_broadcast() {
    let local = Arc::new(LocalStorage::in_memory());
    let hub = StorageCoordinator::new(Arc::clone(&local));
    let (_, a) = hub.subscribe(URL).unwrap();
    let (_, b) = hub.subscribe(URL).unwrap();
    let mut pa = StorageProjection::default();
    let mut pb = StorageProjection::default();
    let mut session = SessionStorage::default();
    let operation = |key: &str| StorageOperation::Set {
        key: key.into(),
        value: "x".repeat(3 * 1024 * 1024).into(),
    };
    let wa = pa
        .write(StorageAreaKind::Local, URL, operation("a"))
        .unwrap()
        .unwrap();
    let wb = pb
        .write(StorageAreaKind::Local, URL, operation("b"))
        .unwrap()
        .unwrap();
    assert!(hub.apply(&a, &[wa], &mut session).unwrap());
    // Both writes passed their synchronous local quota check, but their union does not.
    assert!(hub.apply(&b, &[wb], &mut session).unwrap());
    assert!(b.take_error().is_some());
    let observed_a = drain(&a, &mut pa);
    let observed_b = drain(&b, &mut pb);
    assert_eq!(observed_a.len(), 1);
    assert_eq!(observed_b.len(), 2);
    assert!(observed_b[1].change.is_none());
    assert_eq!(pa.view().snapshot(), pb.view().snapshot());
    assert_eq!(pb.view().snapshot(), local.snapshot(URL).unwrap());
    assert!(pb.view().get(&"b".into()).is_none());
    let retry = pb
        .write(StorageAreaKind::Local, URL, StorageOperation::Clear)
        .unwrap()
        .unwrap();
    assert!(hub.apply(&b, &[retry], &mut session).unwrap());
    drain(&b, &mut pb);
    assert!(pb.view().is_empty());
}

#[test]
fn failed_persistence_sends_only_rejection_ack_and_allows_the_next_write() {
    let (directory, path) = crate::storage::tests::unique_storage("sync-disk-failure");
    let local = Arc::new(LocalStorage::open(&path).unwrap());
    let hub = StorageCoordinator::new(Arc::clone(&local));
    let (_, a) = hub.subscribe(URL).unwrap();
    let (_, b) = hub.subscribe(URL).unwrap();
    let mut projection = StorageProjection::default();
    let mut session = SessionStorage::default();
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::create_dir_all(&temporary).unwrap();
    let write = projection
        .write(
            StorageAreaKind::Local,
            URL,
            set(1, "rejected").mutation.operation,
        )
        .unwrap()
        .unwrap();
    assert!(hub.apply(&a, &[write], &mut session).unwrap());
    assert!(a.take_error().is_some());
    assert!(!b.has_pending());
    let updates = drain(&a, &mut projection);
    assert_eq!(updates.len(), 1);
    assert!(updates[0].change.is_none());
    assert!(projection.view().is_empty());
    assert!(local.snapshot(URL).unwrap().entries.is_empty());
    std::fs::remove_dir(&temporary).unwrap();
    let retry = projection
        .write(
            StorageAreaKind::Local,
            URL,
            set(2, "accepted").mutation.operation,
        )
        .unwrap()
        .unwrap();
    assert!(hub.apply(&a, &[retry], &mut session).unwrap());
    drain(&a, &mut projection);
    assert_eq!(b.take_if(|_| true).unwrap().change.unwrap().old_value, None);
    assert_eq!(
        LocalStorage::open(&path).unwrap().snapshot(URL).unwrap(),
        projection.view().snapshot()
    );
    drop(hub);
    drop(local);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn old_values_count_toward_delivery_bytes_and_pressure_is_recoverable() {
    let local = Arc::new(LocalStorage::in_memory());
    let hub = StorageCoordinator::new(Arc::clone(&local));
    let (_, a) = hub.subscribe(URL).unwrap();
    let (_, b) = hub.subscribe(URL).unwrap();
    let mut session = SessionStorage::default();
    let values = ["a".repeat(4 * 1024 * 1024), "b".repeat(4 * 1024 * 1024)];
    // 4 MiB for the insertion, then 8 MiB for each old/new pair: 28 MiB queued.
    for sequence in 1..=4 {
        assert!(
            hub.apply(
                &a,
                &[set(sequence, &values[sequence as usize % 2])],
                &mut session
            )
            .unwrap()
        );
        a.take_if(|_| true).unwrap();
    }
    let next = set(5, &values[1]);
    let before = local.snapshot(URL).unwrap();
    assert!(
        !hub.apply(&a, std::slice::from_ref(&next), &mut session)
            .unwrap()
    );
    assert_eq!(before, local.snapshot(URL).unwrap());
    // Drain two records: removing only the first still leaves URL/record overhead over 32 MiB.
    b.take_if(|_| true).unwrap();
    b.take_if(|_| true).unwrap();
    assert!(hub.apply(&a, &[next], &mut session).unwrap());
    assert_eq!(local.snapshot(URL).unwrap().version, before.version + 1);
}

#[test]
fn sequence_gaps_and_replay_do_not_commit_or_consume_the_next_sequence() {
    let local = Arc::new(LocalStorage::in_memory());
    let hub = StorageCoordinator::new(Arc::clone(&local));
    let (_, a) = hub.subscribe(URL).unwrap();
    let mut session = SessionStorage::default();
    assert!(hub.apply(&a, &[set(2, "gap")], &mut session).is_err());
    assert!(local.snapshot(URL).unwrap().entries.is_empty());
    assert!(hub.apply(&a, &[set(1, "first")], &mut session).unwrap());
    let before = local.snapshot(URL).unwrap();
    assert!(hub.apply(&a, &[set(1, "replay")], &mut session).is_err());
    assert_eq!(local.snapshot(URL).unwrap(), before);
    assert!(hub.apply(&a, &[set(2, "next")], &mut session).unwrap());
}
