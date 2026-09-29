use super::*;

#[test]
fn explicit_no_suppresses_repeated_prompts_only_for_that_origin_and_device() {
    let tab = TabId::allocate();
    let mut coordinator = CaptureCoordinator::default();
    let denied = context(tab, 1, 7, 13, "https://a.example/", camera());
    let (ticket, _) = prompt(coordinator.begin(denied.clone(), true).unwrap());
    assert_eq!(
        coordinator.complete_prompt(ticket, &denied, true, Some(false), None),
        Err(CaptureFailure::NotAllowed)
    );
    let camera_again = context(tab, 1, 7, 14, "https://a.example/", camera());
    assert_eq!(
        coordinator.begin(camera_again, true),
        Err(CaptureFailure::NotAllowed)
    );
    let microphone_same_origin = context(tab, 1, 7, 15, "https://a.example/", microphone());
    assert_eq!(
        prompt(coordinator.begin(microphone_same_origin, true).unwrap()).1,
        microphone()
    );
    let camera_other_origin = context(tab, 1, 7, 16, "https://b.example/", camera());
    assert_eq!(
        prompt(coordinator.begin(camera_other_origin, true).unwrap()).1,
        camera()
    );
}

#[test]
fn focus_or_navigation_retirement_revokes_active_and_blocks_late_attach() {
    let tab = TabId::allocate();
    let mut coordinator = CaptureCoordinator::default();
    let old = context(tab, 1, 7, 13, "https://a.example/", camera());
    let count = Arc::new(AtomicUsize::new(0));
    let (pending, _) = prompt(coordinator.begin(old.clone(), true).unwrap());
    coordinator
        .complete_prompt(pending, &old, true, Some(true), None)
        .unwrap();
    coordinator.retire_tab(tab); // Called by navigation, external focus loss, and tab teardown.
    assert_eq!(
        coordinator.attach(pending, &old, true, lease(&count)),
        Err(CaptureFailure::Stale)
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);

    let current = context(tab, 2, 8, 14, "https://a.example/", camera());
    let (active, _) = prompt(coordinator.begin(current.clone(), true).unwrap());
    coordinator
        .complete_prompt(active, &current, true, Some(true), None)
        .unwrap();
    coordinator
        .attach(active, &current, true, lease(&count))
        .unwrap();
    coordinator.retire_tab(tab);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert_eq!(coordinator.active_kinds(active), None);
}
