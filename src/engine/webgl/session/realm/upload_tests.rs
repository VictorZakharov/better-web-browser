//! The native owner must observe bytes and binding mutations in one FIFO.
use super::*;

fn buffer(contexts: &mut Contexts, id: u32) -> i64 {
    let buffer = contexts
        .execute(id, r#"{"op":"createBuffer"}"#, None)
        .as_i64()
        .unwrap();
    contexts.execute(
        id,
        &format!(r#"{{"op":"bindBuffer","i":[34962,{buffer}]}}"#),
        None,
    );
    buffer
}

fn context(contexts: &mut Contexts) -> u32 {
    contexts
        .create(2, 2, r#"{"api":"webgl2","antialias":false}"#)
        .unwrap()
}

fn bytes(contexts: &mut Contexts, id: u32, count: usize) -> Vec<u8> {
    let PixelReply::Bytes(bytes) = contexts.read_pixels(
        id,
        &format!(r#"{{"op":"getBufferSubData","i":[34962,0,{count}]}}"#),
        None,
    ) else {
        panic!("buffer observation")
    };
    bytes
}

#[test]
fn borrowed_upload_is_copied_at_admission_and_owned_upload_remains_ordered() {
    let mut contexts = Contexts::default();
    let id = context(&mut contexts);
    buffer(&mut contexts, id);
    let mut source = vec![1, 2, 3, 4, 5, 6, 7, 8];
    contexts.execute(
        id,
        r#"{"op":"bufferData","i":[34962,8,35044]}"#,
        Some(&source),
    );
    source.fill(0);
    contexts.execute_owned(
        id,
        r#"{"op":"bufferSubData","i":[34962,4]}"#,
        Some(vec![9, 10, 11, 12]),
    );
    assert!(
        !contexts.pending.upload_fits(4 * 1024 * 1024),
        "uploads should remain owned/unsent until an observation or bound"
    );
    assert_eq!(bytes(&mut contexts, id, 8), [1, 2, 3, 4, 9, 10, 11, 12]);
    assert!(contexts.inflight.is_none());
    assert_eq!(contexts.execute(id, r#"{"op":"getError"}"#, None), json!(0));
}

#[test]
fn upload_error_observations_keep_native_validation_and_later_work() {
    let mut contexts = Contexts::default();
    let id = context(&mut contexts);
    buffer(&mut contexts, id);
    contexts.execute_owned(
        id,
        r#"{"op":"bufferData","i":[34962,4,35044]}"#,
        Some(vec![1; 4]),
    );
    contexts.execute_owned(
        id,
        r#"{"op":"bufferSubData","i":[34962,3]}"#,
        Some(vec![2; 4]),
    );
    contexts.execute_owned(
        id,
        r#"{"op":"bufferSubData","i":[34962,0]}"#,
        Some(vec![3; 4]),
    );
    assert_eq!(
        contexts.execute(id, r#"{"op":"getError"}"#, None),
        json!(gl::INVALID_VALUE)
    );
    assert_eq!(bytes(&mut contexts, id, 4), [3; 4]);
    contexts.execute_owned(
        id,
        r#"{"op":"bufferData","op":"createTexture"}"#,
        Some(vec![0; 4]),
    );
    assert_eq!(
        contexts.execute(id, r#"{"op":"getError"}"#, None),
        json!(gl::INVALID_VALUE)
    );
    assert_eq!(contexts.execute(id, r#"{"op":"getError"}"#, None), json!(0));
}

#[test]
fn byte_bound_drains_prior_uploads_without_reordering_the_next_payload() {
    let mut contexts = Contexts::default();
    let id = context(&mut contexts);
    buffer(&mut contexts, id);
    for value in 0..17_u8 {
        contexts.execute_owned(
            id,
            r#"{"op":"bufferData","i":[34962,262144,35044]}"#,
            Some(vec![value; 256 * 1024]),
        );
    }
    assert!(
        contexts.inflight.is_some(),
        "the bounded byte batch must be submitted"
    );
    assert_eq!(bytes(&mut contexts, id, 4), [16; 4]);
    assert!(contexts.inflight.is_none());
}

#[test]
fn context_retirement_drops_only_its_unsent_payloads_and_keeps_peers() {
    let mut contexts = Contexts::default();
    let first = context(&mut contexts);
    let second = context(&mut contexts);
    buffer(&mut contexts, first);
    buffer(&mut contexts, second);
    contexts.execute_owned(
        first,
        r#"{"op":"bufferData","i":[34962,4,35044]}"#,
        Some(vec![1; 4]),
    );
    contexts.execute_owned(
        second,
        r#"{"op":"bufferData","i":[34962,4,35044]}"#,
        Some(vec![2; 4]),
    );
    contexts.remove(first);
    assert_eq!(bytes(&mut contexts, second, 4), [2; 4]);
    assert_eq!(
        contexts.execute(first, r#"{"op":"getError"}"#, None),
        json!({"lost":true})
    );
}
