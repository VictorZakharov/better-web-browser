use super::*;

fn request(owner: (TabId, DocumentId), now: Instant, millis: u64) -> Option<Wakeup> {
    Some(Wakeup {
        owner,
        deadline: now + Duration::from_millis(millis),
    })
}

fn owner() -> (TabId, DocumentId) {
    (TabId::first(), DocumentId::new(1).unwrap())
}

#[test]
fn repeated_input_does_not_restart_the_earliest_opportunity() {
    let now = Instant::now();
    let mut wake = RuntimeWakeup::default();
    assert!(wake.update(request(owner(), now, 16), now, |_| true, || {}));
    for millis in 1..16 {
        assert!(wake.update(
            request(owner(), now, millis + 16),
            now,
            |_| panic!("later input must preserve the installed timer"),
            || {}
        ));
    }
    assert!(!wake.due(Some(owner()), now + Duration::from_millis(15)));
    assert!(wake.due(Some(owner()), now + Duration::from_millis(16)));
}

#[test]
fn earlier_work_replaces_the_timer_and_failed_install_preserves_it() {
    let now = Instant::now();
    let mut wake = RuntimeWakeup::default();
    assert!(wake.update(request(owner(), now, 50), now, |_| true, || {}));
    assert!(!wake.update(request(owner(), now, 5), now, |_| false, || {}));
    assert!(!wake.due(Some(owner()), now + Duration::from_millis(5)));
    assert!(wake.update(
        request(owner(), now, 10),
        now,
        |delay| {
            assert_eq!(delay, Duration::from_millis(10));
            true
        },
        || {}
    ));
    assert!(wake.due(Some(owner()), now + Duration::from_millis(10)));
}

#[test]
fn replacing_document_cannot_reuse_an_old_owner_deadline() {
    let now = Instant::now();
    let mut wake = RuntimeWakeup::default();
    assert!(wake.update(request(owner(), now, 1), now, |_| true, || {}));
    let replacement = (owner().0, DocumentId::new(2).unwrap());
    assert!(!wake.due(Some(replacement), now + Duration::from_millis(2)));
    let mut installed = false;
    assert!(wake.update(
        request(replacement, now, 30),
        now,
        |_| {
            installed = true;
            true
        },
        || {}
    ));
    assert!(installed);
    assert!(!wake.due(Some(owner()), now + Duration::from_millis(40)));
    assert!(wake.due(Some(replacement), now + Duration::from_millis(40)));
}

#[test]
fn stopping_clears_due_state_even_with_a_queued_native_timer_message() {
    let now = Instant::now();
    let mut wake = RuntimeWakeup::default();
    assert!(wake.update(request(owner(), now, 0), now, |_| true, || {}));
    let mut stopped = false;
    assert!(wake.update(None, now, |_| unreachable!(), || stopped = true));
    assert!(stopped);
    assert!(!wake.due(Some(owner()), now));
    assert!(!wake.due(None, now));
}
