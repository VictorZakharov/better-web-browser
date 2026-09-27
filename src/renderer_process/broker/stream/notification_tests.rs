use super::*;
use crate::renderer_protocol::NotificationPermission;

fn update(document: DocumentId, request_id: u64, event: NotificationEvent) -> NotificationUpdate {
    NotificationUpdate {
        document,
        request_id,
        event,
    }
}

#[test]
fn permission_and_terminal_events_survive_a_full_bulk_stream() {
    let document = DocumentId::new(7).unwrap();
    let (bulk_tx, bulk_rx) = mpsc::sync_channel(1);
    let (required_tx, required_rx) = mpsc::sync_channel(3);
    let overflow = Arc::new(AtomicBool::new(false));
    let sink = NotificationUpdateSink::new(
        document,
        bulk_tx,
        required_tx,
        Arc::clone(&overflow),
        super::super::wake::BrokerWake::default(),
    );
    sink.try_send(update(document, 1, NotificationEvent::Shown))
        .unwrap();
    sink.try_send(update(
        document,
        2,
        NotificationEvent::Permission(NotificationPermission::Granted),
    ))
    .unwrap();
    sink.try_send(update(document, 3, NotificationEvent::Error))
        .unwrap();
    sink.try_send(update(document, 4, NotificationEvent::Closed))
        .unwrap();
    assert!(!overflow.load(Ordering::Acquire));
    assert!(matches!(
        bulk_rx.try_recv(),
        Ok(FetchStreamEvent::Notification(_))
    ));
    assert_eq!(required_rx.try_recv().unwrap().request_id, 2);
    assert_eq!(required_rx.try_recv().unwrap().request_id, 3);
    assert_eq!(required_rx.try_recv().unwrap().request_id, 4);
}

#[test]
fn required_overflow_fails_closed_but_optional_events_remain_lossy() {
    let document = DocumentId::new(7).unwrap();
    let (bulk_tx, _bulk_rx) = mpsc::sync_channel(1);
    let (required_tx, _required_rx) = mpsc::sync_channel(1);
    let overflow = Arc::new(AtomicBool::new(false));
    let sink = NotificationUpdateSink::new(
        document,
        bulk_tx,
        required_tx,
        Arc::clone(&overflow),
        super::super::wake::BrokerWake::default(),
    );
    sink.try_send(update(document, 1, NotificationEvent::Shown))
        .unwrap();
    assert!(
        sink.try_send(update(document, 2, NotificationEvent::Clicked))
            .is_err()
    );
    assert!(!overflow.load(Ordering::Acquire));
    sink.try_send(update(
        document,
        3,
        NotificationEvent::Permission(NotificationPermission::Denied),
    ))
    .unwrap();
    assert!(
        sink.try_send(update(document, 4, NotificationEvent::Error))
            .is_err()
    );
    assert!(overflow.load(Ordering::Acquire));
}
