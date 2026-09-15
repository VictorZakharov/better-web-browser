use super::*;
use std::sync::{Arc, mpsc};
use std::time::Duration;

#[test]
fn nonblocking_credit_parks_without_spending_bytes_and_rejects_bad_offsets() {
    let flow = FetchFlow::default();
    let document = DocumentId::new(3).unwrap();
    flow.register(document, 1, true).unwrap();
    assert!(
        flow.try_reserve(document, 1, 0, MAX_FETCH_STREAM_WINDOW_BYTES)
            .unwrap()
    );
    assert!(!flow.has_capacity(document, 1, 1).unwrap());
    assert!(
        !flow
            .try_reserve(document, 1, MAX_FETCH_STREAM_WINDOW_BYTES as u32, 1)
            .unwrap()
    );
    assert!(flow.try_reserve(document, 1, 0, 1).is_err());
    assert_eq!(
        flow.state.lock().unwrap().in_flight,
        MAX_FETCH_STREAM_WINDOW_BYTES
    );
    flow.consume(document, 1, 1).unwrap();
    assert!(flow.has_capacity(document, 1, 1).unwrap());
    assert!(
        flow.try_reserve(document, 1, MAX_FETCH_STREAM_WINDOW_BYTES as u32, 1)
            .unwrap()
    );
    flow.retire(document, 1);
    assert!(flow.has_capacity(document, 1, 1).is_err());
    assert_eq!(flow.state.lock().unwrap().in_flight, 0);
}

#[test]
fn slow_consumer_blocks_producer_until_credit_and_cancel_wakes_waiters() {
    let flow = Arc::new(FetchFlow::default());
    let document = DocumentId::new(1).unwrap();
    flow.register(document, 1, true).unwrap();
    flow.reserve(document, 1, 0, MAX_FETCH_STREAM_WINDOW_BYTES)
        .unwrap();
    let (done, receiver) = mpsc::channel();
    let producer = Arc::clone(&flow);
    let thread = std::thread::spawn(move || {
        done.send(producer.reserve(document, 1, MAX_FETCH_STREAM_WINDOW_BYTES as u32, 1))
            .unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());
    flow.consume(document, 1, 1).unwrap();
    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    thread.join().unwrap();
    assert!(flow.consume(document, 1, 0).is_err());
    assert!(
        flow.consume(document, 1, MAX_FETCH_STREAM_WINDOW_BYTES as u32 + 2)
            .is_err()
    );
    let (done, receiver) = mpsc::channel();
    let producer = Arc::clone(&flow);
    let thread = std::thread::spawn(move || {
        done.send(producer.reserve(document, 1, MAX_FETCH_STREAM_WINDOW_BYTES as u32 + 1, 1))
            .unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());
    flow.retire(document, 1);
    assert!(
        receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_err()
    );
    thread.join().unwrap();
    assert_eq!(flow.state.lock().unwrap().in_flight, 0);
}

#[test]
fn aggregate_credit_is_bounded_and_navigation_releases_all_producers() {
    let flow = Arc::new(FetchFlow::default());
    let document = DocumentId::new(2).unwrap();
    let count = MAX_FETCH_STREAM_IN_FLIGHT_BYTES / MAX_FETCH_STREAM_WINDOW_BYTES;
    for id in 1..=count as u64 {
        flow.register(document, id, true).unwrap();
        flow.reserve(document, id, 0, MAX_FETCH_STREAM_WINDOW_BYTES)
            .unwrap();
    }
    let next = count as u64 + 1;
    flow.register(document, next, true).unwrap();
    let (done, receiver) = mpsc::channel();
    let producer = Arc::clone(&flow);
    let thread = std::thread::spawn(move || {
        done.send(producer.reserve(document, next, 0, 1)).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());
    flow.clear();
    assert!(
        receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_err()
    );
    thread.join().unwrap();
    assert_eq!(flow.state.lock().unwrap().in_flight, 0);
}
