use super::*;

#[test]
fn primitive_legacy_messages_keep_their_serialized_contract() {
    for text in ["true", "42", r#""text""#, r#"{"t":"undefined"}"#] {
        let message = WorkerMessage::from(text);
        assert_eq!(message.as_str(), text);
        assert_eq!(message, text);
        assert_eq!(message.capacity(), text.len());
        assert_eq!(message.binary_count(), 0);
        assert!(message.binary("anything", 0).is_none());
    }
}

#[test]
fn envelope_clones_share_immutable_payload_and_charge_binary_storage() {
    let mut lease = begin().unwrap();
    lease.grow(4 + BINARY_ENTRY_CHARGE).unwrap();
    let bytes: BinaryBytes = Arc::new(vec![17, 42, 93, 255]);
    let message = WorkerMessage::from_parts(
        "metadata".into(),
        "private nonce".into(),
        vec![bytes.clone()],
        lease,
    )
    .unwrap();
    let copy = message.clone();
    assert!(Arc::ptr_eq(&message.0, &copy.0));
    assert!(message.capacity() > message.as_str().len() + 4);
    assert!(Arc::ptr_eq(
        &message.binary("private nonce", 0).unwrap(),
        &bytes
    ));
    assert!(message.binary("wrong nonce", 0).is_none());
    assert!(message.binary("private nonce", 1).is_none());
    assert_ne!(
        message, "metadata",
        "binary-bearing messages are not legacy strings"
    );
    let diagnostics = format!("{message:?}");
    assert!(!diagnostics.contains("private nonce") && !diagnostics.contains("metadata\""));
}

#[test]
fn message_storage_is_send_and_sync_but_never_contains_v8_realm_handles() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<WorkerMessage>();
    let message = WorkerMessage::from("true");
    assert_eq!(std::thread::spawn(move || message).join().unwrap(), "true");
}
