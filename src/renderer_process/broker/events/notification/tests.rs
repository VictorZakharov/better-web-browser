use super::super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn diagnostic() -> RendererEvent {
    RendererEvent::Diagnostic {
        code: 1,
        text: "ready".into(),
    }
}

fn watch(receiver: &EventReceiver) -> Arc<AtomicUsize> {
    let count = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&count);
    receiver.set_notifier(move || {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    count
}

#[test]
fn burst_posts_once_and_partial_drain_requires_low_priority_continuation() {
    let (sender, receiver) = bounded();
    let count = watch(&receiver);
    for _ in 0..64 {
        sender.try_send(diagnostic()).unwrap();
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    for _ in 0..32 {
        receiver.try_recv().unwrap();
    }
    assert!(receiver.finish_drain());
    sender.try_send(diagnostic()).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1, "no posted-message storm");
    while receiver.try_recv().is_ok() {}
    assert!(!receiver.finish_drain());
    sender.try_send(diagnostic()).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2, "idle queue wakes again");
}

#[test]
fn registration_wakes_already_queued_events_and_runs_outside_queue_lock() {
    let (sender, receiver) = bounded();
    sender.try_send(diagnostic()).unwrap();
    let queue = Arc::clone(&receiver.queue);
    let (sent, observed) = mpsc::channel();
    receiver.set_notifier(move || {
        // Inspect, but do not consume, the queue from the callback.
        let unlocked = queue.state.try_lock().is_ok();
        sent.send(unlocked).unwrap();
    });
    assert!(observed.recv().unwrap());
    receiver.try_recv().unwrap();
    assert!(!receiver.finish_drain());
    sender.try_send(diagnostic()).unwrap();
    assert!(observed.recv().unwrap());
}

#[test]
fn arrival_during_handling_is_not_lost_and_does_not_post_another_wake() {
    let (sender, receiver) = bounded();
    let count = watch(&receiver);
    sender.try_send(diagnostic()).unwrap();
    receiver.try_recv().unwrap();
    // The queue is temporarily empty while the consumer handles its local batch.
    sender.try_send(diagnostic()).unwrap();
    assert!(receiver.finish_drain());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    receiver.try_recv().unwrap();
    assert!(!receiver.finish_drain());
    sender.try_send(diagnostic()).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn producer_racing_rearm_is_either_backlog_or_a_new_wake() {
    let (sender, receiver) = bounded();
    let count = watch(&receiver);
    let (go, wait) = mpsc::channel();
    let (done, completed) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        while wait.recv().is_ok() {
            sender.try_send(diagnostic()).unwrap();
            done.send(()).unwrap();
        }
    });
    for _ in 0..200 {
        let before = count.load(Ordering::SeqCst);
        go.send(()).unwrap();
        let backlog = receiver.finish_drain();
        completed.recv().unwrap();
        assert!(backlog || count.load(Ordering::SeqCst) > before);
        receiver.try_recv().unwrap();
        assert!(!receiver.finish_drain());
    }
    drop(go);
    producer.join().unwrap();
}

#[test]
fn failed_delivery_can_be_drained_by_fallback_without_losing_rearm() {
    let (sender, receiver) = bounded();
    // Simulate PostMessage failing: no message delivered to the consumer.
    receiver.set_notifier(|| {});
    sender.try_send(diagnostic()).unwrap();
    receiver.try_recv().unwrap(); // health timer fallback
    assert!(!receiver.finish_drain());
    let count = watch(&receiver);
    sender.try_send(diagnostic()).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn close_releases_notifier_and_rejects_later_registration() {
    let (sender, receiver) = bounded();
    let count = watch(&receiver);
    assert_eq!(Arc::strong_count(&count), 2);
    receiver.close();
    assert_eq!(Arc::strong_count(&count), 1);
    receiver.set_notifier(|| panic!("closed receiver must not notify"));
    assert!(sender.try_send(diagnostic()).is_err());
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn lossless_fetch_delivery_uses_the_same_notifier() {
    let (sender, receiver) = bounded();
    let count = watch(&receiver);
    let document = crate::renderer_protocol::DocumentId::new(1).unwrap();
    sender
        .send(RendererEvent::FetchBatch {
            document,
            requests: Vec::new(),
        })
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    receiver.try_recv().unwrap();
    receiver.finish_drain();
    sender
        .send(RendererEvent::FetchAbort {
            document,
            request_id: 1,
        })
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
