//! Deterministic reply/deadline tests; no GL driver or sleeps are required.
use super::*;

fn pending(deadline: Instant) -> (mpsc::SyncSender<Reply>, Submitted) {
    let (sender, incoming) = mpsc::sync_channel(1);
    (sender, Submitted { incoming, deadline })
}

#[test]
fn polling_never_invents_a_reply_or_extends_the_original_deadline() {
    let deadline = Instant::now() + Duration::from_secs(5);
    let (sender, submitted) = pending(deadline);
    for _ in 0..100 {
        assert!(submitted.poll().is_none());
        assert_eq!(submitted.deadline, deadline);
    }
    sender
        .send(Reply::Batch(vec![17]))
        .unwrap_or_else(|_| panic!("receiver"));
    assert!(matches!(submitted.poll(), Some(Some(Reply::Batch(ids))) if ids == [17]));
}

#[test]
fn an_already_available_reply_can_be_consumed_without_an_extra_wait() {
    let (sender, submitted) = pending(Instant::now() - Duration::from_secs(1));
    sender
        .send(Reply::Batch(Vec::new()))
        .unwrap_or_else(|_| panic!("receiver"));
    assert!(matches!(submitted.wait(), Some(Reply::Batch(ids)) if ids.is_empty()));
}

#[test]
fn stalled_and_disconnected_requests_fail_closed() {
    let (sender, submitted) = pending(Instant::now() - Duration::from_secs(1));
    assert!(matches!(submitted.poll(), Some(None)));
    assert!(submitted.wait().is_none());
    drop(sender);
    let (sender, submitted) = pending(Instant::now() + Duration::from_secs(5));
    drop(sender);
    assert!(matches!(submitted.poll(), Some(None)));
    assert!(submitted.wait().is_none());
}

#[test]
fn cancelling_an_outstanding_request_disconnects_the_owners_reply() {
    let (sender, submitted) = pending(Instant::now() + Duration::from_secs(5));
    drop(submitted);
    assert!(sender.send(Reply::Batch(vec![1, 2])).is_err());
}
