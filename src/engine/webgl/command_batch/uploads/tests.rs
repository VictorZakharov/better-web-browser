use super::*;

#[test]
fn upload_admission_is_canonical_reply_free_and_bounded() {
    for operation in [
        "bufferData",
        "bufferSubData",
        "texImage2D",
        "texSubImage2D",
        "texImage3D",
        "texSubImage3D",
        "compressedTexImage2D",
        "compressedTexSubImage2D",
        "compressedTexImage3D",
        "compressedTexSubImage3D",
    ] {
        let source = format!(r#"{{"op":"{operation}","i":[],"f":[],"text":""}}"#);
        assert!(Pending::upload_candidate(&source, 0));
        assert!(Pending::upload_candidate(&source, MAX_QUEUED_UPLOAD));
        assert!(!Pending::upload_candidate(&source, MAX_QUEUED_UPLOAD + 1));
    }
    for source in [
        r#"{"op":"getBufferSubData"}"#,
        r#"{"op":"readPixels"}"#,
        r#"{"op":"createTexture"}"#,
        r#"{"op":"finish"}"#,
        r#"{"op":"clear"}"#,
        r#"{"op": "texSubImage2D"}"#,
        r#"{"i":[],"op":"texSubImage2D"}"#,
        r#"{"op":"texSubImage2Dsuffix"}"#,
        r#"{"op":"texSubImage2D"suffix:0}"#,
        r#"{"op":"getError","text":"texSubImage2D"}"#,
    ] {
        assert!(!Pending::upload_candidate(source, 4), "{source}");
    }
    let source = format!(
        r#"{{"op":"bufferData","text":"{}"}}"#,
        "a".repeat(MAX_COMMAND_BYTES)
    );
    assert!(!Pending::upload_candidate(&source, 4));
}

#[test]
fn byte_budget_drain_and_retirement_preserve_surviving_fifo_and_release_accounting() {
    let mut pending = Pending::default();
    let command = r#"{"op":"bufferSubData","i":[34962,0]}"#;
    for index in 0..16 {
        pending.push_upload(index % 2 + 1, command, vec![index as u8; MAX_QUEUED_UPLOAD]);
    }
    assert!(pending.upload_fits(0));
    assert!(!pending.upload_fits(1));
    assert!(!pending.upload_fits(usize::MAX));
    pending.remove(1);
    assert!(pending.upload_fits(MAX_PENDING_BYTES / 2));
    assert!(!pending.upload_fits(MAX_PENDING_BYTES / 2 + 1));
    let commands = pending.take();
    assert!(pending.upload_fits(MAX_PENDING_BYTES));
    for (index, (id, entry)) in commands.iter().enumerate() {
        assert_eq!(*id, 2);
        let Entry::Upload(source, bytes) = entry else {
            panic!("owned upload")
        };
        assert_eq!(source, command);
        assert!(bytes.iter().all(|byte| *byte == (index * 2 + 1) as u8));
    }
    pending.push_upload(1, command, vec![9; MAX_QUEUED_UPLOAD]);
    pending.clear();
    assert!(pending.take().is_empty() && pending.upload_fits(MAX_PENDING_BYTES));
}

#[test]
fn upload_payloads_count_towards_the_same_command_capacity() {
    let mut pending = Pending::default();
    for index in 0..MAX_COMMANDS {
        if index % 2 == 0 {
            pending.push_upload(1, r#"{"op":"bufferData"}"#, vec![0; 4]);
        } else {
            pending.push(1, r#"{"op":"clear"}"#);
        }
        assert_eq!(pending.full(), index + 1 == MAX_COMMANDS);
    }
    assert_eq!(pending.take().len(), MAX_COMMANDS);
    assert!(!pending.full());
}
