use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn context(
    tab: TabId,
    document: u64,
    session: u64,
    request: u64,
    url: &str,
    kinds: CaptureKinds,
) -> CaptureContext {
    CaptureContext {
        key: CaptureKey {
            tab,
            document: DocumentId::new(document).unwrap(),
            renderer_session: session,
            client_id: 0,
            request_id: request,
        },
        origin: Origin::parse(url).unwrap(),
        kinds,
    }
}

fn camera() -> CaptureKinds {
    CaptureKinds {
        camera: true,
        microphone: false,
    }
}

fn microphone() -> CaptureKinds {
    CaptureKinds {
        camera: false,
        microphone: true,
    }
}

fn lease(count: &Arc<AtomicUsize>) -> CaptureLease {
    let count = Arc::clone(count);
    CaptureLease::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
    })
}

fn prompt(admission: CaptureAdmission) -> (CaptureTicket, CaptureKinds) {
    match admission {
        CaptureAdmission::Prompt(ticket, kinds) => (ticket, kinds),
        CaptureAdmission::Ready(_) => panic!("expected prompt"),
    }
}

#[test]
fn camera_and_microphone_decisions_are_separate_and_origin_exact() {
    let tab = TabId::allocate();
    let mut coordinator = CaptureCoordinator::default();
    let first = context(tab, 1, 11, 21, "https://a.example/page", camera());
    let (camera_ticket, requested) = prompt(coordinator.begin(first.clone(), true).unwrap());
    assert_eq!(requested, camera());
    coordinator
        .complete_prompt(camera_ticket, &first, true, Some(true), None)
        .unwrap();
    let stopped = Arc::new(AtomicUsize::new(0));
    coordinator
        .attach(camera_ticket, &first, true, lease(&stopped))
        .unwrap();

    let mic = context(tab, 1, 11, 22, "https://a.example/other", microphone());
    let (mic_ticket, requested) = prompt(coordinator.begin(mic.clone(), true).unwrap());
    assert_eq!(requested, microphone());
    coordinator
        .complete_prompt(mic_ticket, &mic, true, None, Some(false))
        .unwrap_err();
    assert_eq!(
        coordinator.begin(mic, true),
        Err(CaptureFailure::NotAllowed)
    );

    let other = context(tab, 1, 11, 23, "https://b.example/", camera());
    assert_eq!(prompt(coordinator.begin(other, true).unwrap()).1, camera());
    let again = context(tab, 1, 11, 24, "https://a.example/", camera());
    assert!(matches!(
        coordinator.begin(again, true),
        Ok(CaptureAdmission::Ready(_))
    ));
    assert_eq!(stopped.load(Ordering::SeqCst), 0);
}

#[test]
fn stale_prompt_cannot_grant_a_reused_request_key() {
    let tab = TabId::allocate();
    let mut coordinator = CaptureCoordinator::default();
    let request = context(tab, 1, 7, 13, "https://a.example/", camera());
    let (old, _) = prompt(coordinator.begin(request.clone(), true).unwrap());
    coordinator.retire_tab(tab);
    let (current, _) = prompt(coordinator.begin(request.clone(), true).unwrap());
    assert_ne!(old, current);
    assert_eq!(
        coordinator.complete_prompt(old, &request, true, Some(true), None),
        Err(CaptureFailure::Stale)
    );
    coordinator
        .complete_prompt(current, &request, true, Some(true), None)
        .unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    assert_eq!(
        coordinator.attach(old, &request, true, lease(&count)),
        Err(CaptureFailure::Stale)
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    coordinator
        .attach(current, &request, true, lease(&count))
        .unwrap();
    assert_eq!(coordinator.stop(old), Err(CaptureFailure::Stale));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    coordinator.stop(current).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn changed_document_session_origin_or_visibility_revokes_late_attach() {
    let tab = TabId::allocate();
    let mut coordinator = CaptureCoordinator::default();
    let original = context(tab, 1, 7, 13, "https://a.example/", camera());
    let count = Arc::new(AtomicUsize::new(0));
    for changed in [
        context(tab, 2, 7, 13, "https://a.example/", camera()),
        context(tab, 1, 8, 13, "https://a.example/", camera()),
        context(tab, 1, 7, 13, "https://b.example/", camera()),
    ] {
        let (ticket, _) = prompt(coordinator.begin(original.clone(), true).unwrap());
        assert_eq!(
            coordinator.complete_prompt(ticket, &changed, true, Some(true), None),
            Err(CaptureFailure::Stale)
        );
        assert_eq!(
            coordinator.attach(ticket, &original, true, lease(&count)),
            Err(CaptureFailure::Stale)
        );
    }
    let (ticket, _) = prompt(coordinator.begin(original.clone(), true).unwrap());
    assert_eq!(
        coordinator.complete_prompt(ticket, &original, false, Some(true), None),
        Err(CaptureFailure::Stale)
    );
    let (ticket, _) = prompt(coordinator.begin(original.clone(), true).unwrap());
    coordinator
        .complete_prompt(ticket, &original, true, Some(true), None)
        .unwrap();
    assert_eq!(
        coordinator.attach(ticket, &original, false, lease(&count)),
        Err(CaptureFailure::Stale)
    );
    assert_eq!(count.load(Ordering::SeqCst), 4);
    assert!(coordinator.permissions.contains_key(&original.origin));
}

#[test]
fn tab_retirement_revokes_only_its_active_grants_and_clears_pending() {
    let first_tab = TabId::allocate();
    let second_tab = TabId::allocate();
    let mut coordinator = CaptureCoordinator::default();
    let count = Arc::new(AtomicUsize::new(0));
    let first = context(first_tab, 1, 7, 13, "https://a.example/", camera());
    let second = context(second_tab, 2, 8, 14, "https://b.example/", microphone());
    let (first_ticket, _) = prompt(coordinator.begin(first.clone(), true).unwrap());
    coordinator
        .complete_prompt(first_ticket, &first, true, Some(true), None)
        .unwrap();
    coordinator
        .attach(first_ticket, &first, true, lease(&count))
        .unwrap();
    let (second_ticket, _) = prompt(coordinator.begin(second.clone(), true).unwrap());
    coordinator
        .complete_prompt(second_ticket, &second, true, None, Some(true))
        .unwrap();
    coordinator
        .attach(second_ticket, &second, true, lease(&count))
        .unwrap();
    let waiting = context(first_tab, 1, 7, 15, "https://c.example/", camera());
    let (waiting_ticket, _) = prompt(coordinator.begin(waiting.clone(), true).unwrap());
    coordinator.retire_tab(first_tab);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(
        coordinator.complete_prompt(waiting_ticket, &waiting, true, Some(true), None),
        Err(CaptureFailure::Stale)
    );
    assert_eq!(coordinator.stop(first_ticket), Err(CaptureFailure::Stale));
    coordinator.stop(second_ticket).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn untrusted_child_hidden_or_empty_requests_never_prompt() {
    let tab = TabId::allocate();
    let mut coordinator = CaptureCoordinator::default();
    let mut secure = context(tab, 1, 7, 13, "https://a.example/", camera());
    assert_eq!(
        coordinator.begin(secure.clone(), false),
        Err(CaptureFailure::NotAllowed)
    );
    secure.key.client_id = 1;
    assert_eq!(
        coordinator.begin(secure, true),
        Err(CaptureFailure::NotAllowed)
    );
    let insecure = context(tab, 1, 7, 13, "http://a.example/", camera());
    assert_eq!(
        coordinator.begin(insecure, true),
        Err(CaptureFailure::NotAllowed)
    );
    let empty = context(
        tab,
        1,
        7,
        13,
        "https://a.example/",
        CaptureKinds {
            camera: false,
            microphone: false,
        },
    );
    assert_eq!(
        coordinator.begin(empty, true),
        Err(CaptureFailure::NotAllowed)
    );
    assert!(coordinator.permissions.is_empty());
    assert!(coordinator.pending.is_empty());
}
