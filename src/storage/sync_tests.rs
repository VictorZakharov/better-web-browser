use super::*;
use std::sync::Arc;

#[path = "sync_edge_tests.rs"]
mod edges;

const URL: &str = "https://example.com/source?query=1#fragment";

fn set(sequence: u64, value: &str) -> StorageWrite {
    StorageWrite {
        sequence,
        source_url: URL.into(),
        mutation: StorageMutation {
            area: StorageAreaKind::Local,
            expected_version: 1,
            operation: StorageOperation::Set {
                key: "key".into(),
                value: value.into(),
            },
        },
    }
}

fn drain(
    subscription: &StorageSubscription,
    projection: &mut StorageProjection,
) -> Vec<StorageUpdate> {
    let mut updates = Vec::new();
    while let Some(update) = subscription.take_if(|_| true) {
        projection.apply(&update).unwrap();
        updates.push(update);
    }
    updates
}

#[test]
fn concurrent_writes_rebase_under_pending_intents_and_converge() {
    let local = Arc::new(LocalStorage::in_memory());
    let hub = StorageCoordinator::new(Arc::clone(&local));
    let (_, a) = hub.subscribe(URL).unwrap();
    let (_, b) = hub.subscribe("https://example.com/other").unwrap();
    let mut pa = StorageProjection::default();
    let mut pb = StorageProjection::default();
    let mut session = SessionStorage::default();
    let write = |p: &mut StorageProjection, value: &str| {
        p.write(
            StorageAreaKind::Local,
            URL,
            set(1, value).mutation.operation,
        )
        .unwrap()
        .unwrap()
    };
    let a1 = write(&mut pa, "a1");
    let a2 = write(&mut pa, "a2");
    let b1 = write(&mut pb, "b");
    assert!(hub.apply(&a, &[a1, a2], &mut session).unwrap());
    let first = b.take_if(|_| true).unwrap();
    pb.apply(&first).unwrap();
    assert_eq!(
        pb.view().get(&"key".into()),
        Some(&"b".into()),
        "remote update must not erase pending local intent"
    );
    assert!(hub.apply(&b, &[b1], &mut session).unwrap());
    let observed_a = drain(&a, &mut pa);
    let observed_b = drain(&b, &mut pb);
    assert_eq!(
        observed_a.iter().filter(|u| u.acknowledgement == 0).count(),
        1
    );
    assert_eq!(observed_b.len(), 2);
    assert_eq!(pa.view().snapshot(), pb.view().snapshot());
    assert_eq!(pa.view().snapshot(), local.snapshot(URL).unwrap());
    assert_eq!(pa.view().get(&"key".into()), Some(&"b".into()));
}

#[test]
fn globally_redundant_write_is_acknowledged_without_broadcast() {
    let hub = StorageCoordinator::new(Arc::new(LocalStorage::in_memory()));
    let (_, a) = hub.subscribe(URL).unwrap();
    let (_, b) = hub.subscribe(URL).unwrap();
    let mut session = SessionStorage::default();
    hub.apply(&a, &[set(1, "same")], &mut session).unwrap();
    a.take_if(|_| true).unwrap();
    b.take_if(|_| true).unwrap();
    hub.apply(&b, &[set(1, "same")], &mut session).unwrap();
    assert!(!a.has_pending());
    let acknowledgement = b.take_if(|_| true).unwrap();
    assert_eq!(acknowledgement.acknowledgement, 1);
    assert_eq!(acknowledgement.version, 2);
    assert!(acknowledgement.change.is_none());
}

#[test]
fn origin_boundaries_session_isolation_and_source_url_are_preserved() {
    let hub = StorageCoordinator::new(Arc::new(LocalStorage::in_memory()));
    let (_, a) = hub.subscribe(URL).unwrap();
    let (_, b) = hub.subscribe("https://example.com/recipient").unwrap();
    let (_, cross) = hub.subscribe("https://example.com:444/").unwrap();
    let mut session = SessionStorage::default();
    hub.apply(&a, &[set(1, "x")], &mut session).unwrap();
    assert_eq!(b.take_if(|_| true).unwrap().source_url, URL);
    assert!(!cross.has_pending());
    a.take_if(|_| true).unwrap();
    let mut write = set(1, "private");
    write.mutation.area = StorageAreaKind::Session;
    hub.apply(&a, &[write], &mut session).unwrap();
    assert!(!b.has_pending());
    assert_eq!(a.take_if(|_| true).unwrap().area, StorageAreaKind::Session);
    let mut forged = set(2, "forged");
    forged.source_url = "https://other.example/".into();
    assert!(hub.apply(&a, &[forged], &mut session).is_err());
    assert!(!a.has_pending());
}

#[test]
fn a_full_recipient_defers_before_commit_and_retry_does_not_lose_a_write() {
    let local = Arc::new(LocalStorage::in_memory());
    let hub = StorageCoordinator::new(Arc::clone(&local));
    let (_, a) = hub.subscribe(URL).unwrap();
    let (_, b) = hub.subscribe(URL).unwrap();
    let mut session = SessionStorage::default();
    let limit = crate::limits::MAX_QUEUED_BROWSER_WRITES as u64;
    for sequence in 1..=limit {
        assert!(
            hub.apply(&a, &[set(sequence, &sequence.to_string())], &mut session)
                .unwrap()
        );
        a.take_if(|_| true).unwrap();
    }
    let before = local.snapshot(URL).unwrap();
    let next = set(limit + 1, "retry");
    assert!(
        !hub.apply(&a, std::slice::from_ref(&next), &mut session)
            .unwrap()
    );
    assert_eq!(local.snapshot(URL).unwrap(), before);
    b.take_if(|_| true).unwrap();
    assert!(hub.apply(&a, &[next], &mut session).unwrap());
    assert_eq!(local.snapshot(URL).unwrap().entries[0].value, "retry");
}

#[test]
fn closing_a_full_recipient_releases_pressure_and_new_documents_get_no_old_events() {
    let local = Arc::new(LocalStorage::in_memory());
    let hub = StorageCoordinator::new(Arc::clone(&local));
    let (_, a) = hub.subscribe(URL).unwrap();
    let (_, old) = hub.subscribe(URL).unwrap();
    let mut session = SessionStorage::default();
    hub.apply(&a, &[set(1, "before")], &mut session).unwrap();
    drop(old);
    let (snapshot, new) = hub.subscribe(URL).unwrap();
    assert_eq!(snapshot, local.snapshot(URL).unwrap());
    assert!(!new.has_pending());
    hub.apply(&a, &[set(2, "after")], &mut session).unwrap();
    assert_eq!(
        new.take_if(|_| true).unwrap().change.unwrap().old_value,
        Some("before".into())
    );
}

#[test]
fn invalid_acknowledgement_does_not_mutate_the_replica() {
    let mut projection = StorageProjection::default();
    projection
        .write(
            StorageAreaKind::Local,
            URL,
            set(1, "pending").mutation.operation,
        )
        .unwrap();
    let before = projection.view().snapshot();
    let wrong = StorageUpdate {
        area: StorageAreaKind::Local,
        version: 1,
        acknowledgement: 2,
        change: None,
        source_url: URL.into(),
    };
    assert!(projection.apply(&wrong).is_err());
    assert_eq!(projection.view().snapshot(), before);
    projection
        .apply(&StorageUpdate {
            acknowledgement: 1,
            ..wrong
        })
        .unwrap();
    assert!(
        projection.view().is_empty(),
        "a rejected intent is retired without a change"
    );
}
