//! Permits follow real native worker lifetimes, not JavaScript session IDs.
use super::*;

fn wait_released(workers: &Arc<AtomicUsize>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while workers.load(Ordering::Acquire) != 0 {
        assert!(
            Instant::now() < deadline,
            "native workers retained after cancellation"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn realm_capacity_is_bounded_and_close_releases_real_workers() {
    let mut codecs = AudioCodecs::default();
    let workers = Arc::clone(&codecs.workers);
    let mut ids = Vec::new();
    for _ in 0..MAX_SESSIONS {
        let id = codecs.start(config(48_000, 1, true), true).unwrap();
        wait(&mut codecs, id).unwrap();
        ids.push(id);
    }
    assert_eq!(workers.load(Ordering::Acquire), MAX_SESSIONS);
    assert!(
        codecs
            .start(config(48_000, 1, true), true)
            .unwrap_err()
            .contains("eight")
    );
    for id in ids {
        codecs.close(id);
        assert!(codecs.poll(id).is_err());
    }
    wait_released(&workers);
    let id = codecs.start(config(48_000, 1, true), true).unwrap();
    wait(&mut codecs, id).unwrap();
    codecs.close(id);
    wait_released(&workers);
}

#[test]
fn dropping_realm_cancels_idle_threads_without_an_explicit_close() {
    let mut codecs = AudioCodecs::default();
    let workers = Arc::clone(&codecs.workers);
    for _ in 0..3 {
        let id = codecs.start(config(48_000, 1, false), false).unwrap();
        wait(&mut codecs, id).unwrap();
    }
    assert_eq!(workers.load(Ordering::Acquire), 3);
    drop(codecs);
    wait_released(&workers);
}

#[test]
fn cancelled_input_result_cannot_reappear_under_a_later_session_id() {
    let mut codecs = AudioCodecs::default();
    let workers = Arc::clone(&codecs.workers);
    let previous = codecs.start(config(48_000, 2, true), true).unwrap();
    wait(&mut codecs, previous).unwrap();
    codecs
        .submit(
            previous,
            Command::Input {
                bytes: tone(60_000, 2, 48_000),
                timestamp: 0,
            },
        )
        .unwrap();
    codecs.close(previous);
    wait_released(&workers);
    let replacement = codecs.start(config(48_000, 1, false), false).unwrap();
    assert!(replacement > previous);
    assert!(wait(&mut codecs, replacement).unwrap().is_empty());
    assert!(codecs.poll(previous).is_err());
    codecs.close(replacement);
    wait_released(&workers);
}

#[test]
fn oversized_commands_are_rejected_without_marking_session_busy() {
    let mut codecs = AudioCodecs::default();
    let id = codecs.start(config(48_000, 1, true), true).unwrap();
    wait(&mut codecs, id).unwrap();
    assert!(
        codecs
            .submit(
                id,
                Command::Input {
                    bytes: vec![0; MAX_INPUT_BYTES + 1],
                    timestamp: 0
                }
            )
            .is_err()
    );
    assert!(!codecs.sessions.get(&id).unwrap().busy);
    codecs
        .submit(
            id,
            Command::Input {
                bytes: tone(960, 1, 48_000),
                timestamp: 0,
            },
        )
        .unwrap();
    assert_eq!(wait(&mut codecs, id).unwrap().len(), 1);
    codecs.close(id);
}

#[test]
fn invalid_configurations_never_reserve_a_native_permit() {
    let mut codecs = AudioCodecs::default();
    let mut cfg = config(48_000, 1, true);
    cfg.codec = "unimplemented".into();
    assert!(codecs.start(cfg, true).is_err());
    assert_eq!(codecs.workers.load(Ordering::Acquire), 0);
    assert!(codecs.sessions.is_empty());
    assert_eq!(codecs.next_id, 0);
}

#[test]
fn native_codec_failures_leave_a_closable_session_and_no_placeholder_output() {
    let mut codecs = AudioCodecs::default();
    let workers = Arc::clone(&codecs.workers);
    let id = codecs.start(config(48_000, 1, false), false).unwrap();
    wait(&mut codecs, id).unwrap();
    codecs
        .submit(
            id,
            Command::Input {
                bytes: vec![255],
                timestamp: 0,
            },
        )
        .unwrap();
    assert!(wait(&mut codecs, id).is_err());
    codecs.close(id);
    wait_released(&workers);
}
