use super::*;

fn owner(document: u64, session_id: u64) -> SpeechOwner {
    SpeechOwner {
        tab: TabId::first(),
        document: DocumentId::new(document).unwrap(),
        session_id,
    }
}

#[test]
fn retirement_is_independent_of_worker_mailbox_and_scoped_to_owner() {
    let service = SpeechSynthesisService::spawn().expect("speech worker thread");
    // Retirement is recorded independently of the bounded worker mailbox. A
    // navigation may therefore revoke audio even when that mailbox is saturated.
    service.retire(owner(5, 10));
    assert!(is_retired(&service.retired, owner(1, 10)));
    assert!(is_retired(&service.retired, owner(5, 10)));
    assert!(!is_retired(&service.retired, owner(6, 10)));
    service.retire(owner(3, 10));
    assert!(is_retired(&service.retired, owner(5, 10)));
    // A renderer retry may reuse its document number, but not its session identity.
    assert!(!is_retired(&service.retired, owner(5, 11)));
    assert!(!is_retired(&service.retired, owner(6, 10)));
    service.retire(owner(5, 11));
    assert!(is_retired(&service.retired, owner(5, 10)));
    assert!(is_retired(&service.retired, owner(5, 11)));
    // A fresh navigation in that renderer session has a higher document generation.
    assert!(!is_retired(&service.retired, owner(6, 11)));
    // Closing a tab cannot retire an unrelated tab using the same session/document numbers.
    let other_tab = SpeechOwner {
        tab: TabId::allocate(),
        ..owner(5, 10)
    };
    assert!(!is_retired(&service.retired, other_tab));
    // Repeated replacement of one tab retains a single high-water entry.
    assert_eq!(service.retired.lock().unwrap().len(), 1);
}

#[test]
fn protocol_text_bounds_preserve_utf8_characters() {
    assert_eq!(bounded_utf8("ééé".into(), 5), "éé");
    assert_eq!(bounded_utf8("voice".into(), 5), "voice");
}

#[test]
fn pausing_one_document_does_not_pause_another_document_without_audio() {
    let first = owner(1, 10);
    let second = SpeechOwner {
        tab: TabId::allocate(),
        ..owner(1, 11)
    };
    let mut states = HashMap::<SpeechOwner, OwnerSpeech>::new();
    states.insert(first, OwnerSpeech::default());
    states.insert(second, OwnerSpeech::default());

    pause_owner(first, states.get_mut(&first).unwrap());
    assert!(states[&first].paused);
    assert!(!states[&second].paused);
    resume_owner(first, states.get_mut(&first).unwrap());
    assert!(!states[&first].paused);
    assert!(!states[&second].paused);
}
