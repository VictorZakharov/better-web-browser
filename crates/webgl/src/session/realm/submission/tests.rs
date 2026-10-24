//! Replies can fail without allowing queued work or stale IDs to escape a realm.
use super::*;
use std::time::Instant;

const FIRST: u32 = u32::MAX - 10;
const SECOND: u32 = u32::MAX - 9;
const RESTORED: u32 = u32::MAX - 8;

fn realm(ids: &[u32]) -> Contexts {
    let mut realm = Contexts::default();
    for &id in ids {
        realm.live.insert(id);
        realm.leases.insert(id, Arc::new(()));
    }
    realm
}

fn stage(realm: &mut Contexts, ids: &[u32], expired: bool) -> mpsc::SyncSender<Reply> {
    let deadline = if expired {
        Instant::now() - Duration::from_secs(1)
    } else {
        Instant::now() + Duration::from_secs(5)
    };
    let (sender, reply) = Submitted::test_pending(deadline);
    realm.inflight = Some(Flight {
        reply,
        ids: ids.iter().copied().collect(),
    });
    sender
}

#[test]
fn a_pending_poll_keeps_one_flight_and_does_not_discard_valid_work() {
    let mut realm = realm(&[FIRST, SECOND]);
    let sender = stage(&mut realm, &[FIRST], false);
    realm.pending.push(SECOND, r#"{"op":"clear","i":[16384]}"#);
    for _ in 0..100 {
        realm.reap_pending(false);
        assert!(realm.inflight.is_some());
        assert_eq!(realm.live.len(), 2);
    }
    sender
        .send(Reply::Batch(Vec::new()))
        .unwrap_or_else(|_| panic!("receiver"));
    realm.reap_pending(false);
    assert!(realm.inflight.is_none());
    assert_eq!(realm.pending.take().len(), 1);
    assert_eq!(realm.live.len(), 2);
}

#[test]
fn reported_loss_removes_unsent_work_for_only_the_submitted_context() {
    let mut realm = realm(&[FIRST, SECOND, RESTORED]);
    let sender = stage(&mut realm, &[FIRST, SECOND], false);
    for id in [FIRST, SECOND, RESTORED, SECOND] {
        realm.pending.push(id, r#"{"op":"clear","i":[16384]}"#);
    }
    sender
        .send(Reply::Batch(vec![FIRST, RESTORED]))
        .unwrap_or_else(|_| panic!("receiver"));
    realm.reap_pending(true);
    assert!(!realm.live.contains(&FIRST));
    assert!(realm.live.contains(&SECOND) && realm.live.contains(&RESTORED));
    let ids: Vec<_> = realm.pending.take().into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, [SECOND, RESTORED, SECOND]);
}

#[test]
fn timeout_disconnection_and_wrong_reply_retire_only_the_flights_ids() {
    for failure in 0..3 {
        let mut realm = realm(&[FIRST, SECOND, RESTORED]);
        let sender = stage(&mut realm, &[FIRST, SECOND], failure == 0);
        realm.pending.push(FIRST, r#"{"op":"clear","i":[16384]}"#);
        realm
            .pending
            .push(RESTORED, r#"{"op":"clear","i":[16384]}"#);
        match failure {
            0 => {}
            1 => drop(sender),
            _ => sender
                .send(Reply::Profiling)
                .unwrap_or_else(|_| panic!("receiver")),
        }
        realm.reap_pending(false);
        assert!(realm.inflight.is_none());
        assert_eq!(realm.live.iter().copied().collect::<Vec<_>>(), [RESTORED]);
        assert_eq!(
            realm
                .pending
                .take()
                .into_iter()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            [RESTORED]
        );
    }
}

#[test]
fn clearing_a_realm_cancels_inflight_and_unsent_batches_without_waiting() {
    let mut realm = realm(&[FIRST, SECOND]);
    let sender = stage(&mut realm, &[FIRST], false);
    realm.pending.push(SECOND, r#"{"op":"clear","i":[16384]}"#);
    realm.clear();
    assert!(realm.inflight.is_none() && realm.pending.take().is_empty());
    assert!(realm.live.is_empty() && realm.leases.is_empty());
    assert!(sender.send(Reply::Batch(Vec::new())).is_err());
}

#[test]
fn retiring_one_context_does_not_cancel_a_peers_inflight_reply() {
    let mut realm = realm(&[FIRST, SECOND]);
    let sender = stage(&mut realm, &[FIRST, SECOND], false);
    realm.remove(FIRST);
    assert!(realm.inflight.is_some());
    realm.live.insert(RESTORED);
    realm.leases.insert(RESTORED, Arc::new(()));
    sender
        .send(Reply::Batch(vec![FIRST]))
        .unwrap_or_else(|_| panic!("receiver"));
    realm.reap_pending(true);
    assert!(realm.live.contains(&SECOND) && realm.live.contains(&RESTORED));
}
