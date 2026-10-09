use super::*;

fn bytes(value: u8, length: usize) -> BinaryBytes {
    Arc::new(vec![value; length])
}

#[test]
fn reader_only_accepts_current_message_nonce_and_each_binary_once() {
    let state = Rc::new(State::default());
    let writer = state.write().unwrap();
    let token = state.append(4, || Ok(bytes(42, 4))).unwrap();
    let message = writer.finish("graph".into()).unwrap();
    assert!(state.consume(&token).is_err());
    let reader = state.read(message).unwrap();
    assert!(state.consume("@breeze-binary/guessed/0").is_err());
    assert_eq!(&*state.consume(&token).unwrap(), &[42; 4]);
    assert!(state.consume(&token).is_err());
    drop(reader);
    assert!(state.consume(&token).is_err());
}

#[test]
fn nested_capture_keeps_outer_and_inner_payloads_separate() {
    let state = Rc::new(State::default());
    let outer = state.write().unwrap();
    let a = state.append(3, || Ok(bytes(1, 3))).unwrap();
    let inner = state.write().unwrap();
    let b = state.append(5, || Ok(bytes(2, 5))).unwrap();
    let inner = inner.finish("inner".into()).unwrap();
    let c = state.append(7, || Ok(bytes(3, 7))).unwrap();
    let outer = outer.finish("outer".into()).unwrap();
    let outer_read = state.read(outer).unwrap();
    assert_eq!(state.consume(&a).unwrap().len(), 3);
    let inner_read = state.read(inner).unwrap();
    assert!(state.consume(&c).is_err());
    assert_eq!(state.consume(&b).unwrap().len(), 5);
    drop(inner_read);
    assert_eq!(state.consume(&c).unwrap().len(), 7);
    drop(outer_read);
}

#[test]
fn receivers_in_other_realms_need_the_actual_native_envelope() {
    let sender = Rc::new(State::default());
    let receiver = Rc::new(State::default());
    let writer = sender.write().unwrap();
    let token = sender.append(1, || Ok(bytes(17, 1))).unwrap();
    let envelope = writer.finish("metadata".into()).unwrap();
    let forged = receiver.read("metadata".into()).unwrap();
    assert!(receiver.consume(&token).is_err());
    drop(forged);
    let valid = receiver.read(envelope).unwrap();
    assert_eq!(&*receiver.consume(&token).unwrap(), &[17]);
    drop(valid);
}

#[test]
fn failed_copy_and_unwinding_release_partial_capture() {
    let state = Rc::new(State::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _writer = state.write().unwrap();
        state.append(1024, || Ok(bytes(7, 1024))).unwrap();
        assert!(state.append(2, || Err("injected copy failure")).is_err());
        panic!("injected getter unwind");
    }));
    assert!(result.is_err());
    assert!(state.writers.borrow().is_empty());
    assert!(!state.poisoned.get());
    state.write().unwrap().finish("next".into()).unwrap();
}

#[test]
fn storage_limit_is_checked_before_the_native_copy() {
    let state = Rc::new(State::default());
    let _writer = state.write().unwrap();
    let copied = Cell::new(false);
    assert!(
        state
            .append(MAX_BINARY_BYTES + 1, || {
                copied.set(true);
                Ok(bytes(0, 1))
            })
            .is_err()
    );
    assert!(!copied.get());
    assert!(
        state
            .append(usize::MAX, || {
                copied.set(true);
                Ok(bytes(0, 1))
            })
            .is_err()
    );
    assert!(!copied.get());
}

#[test]
fn nesting_and_count_limits_do_not_allocate_extra_payloads() {
    let state = Rc::new(State::default());
    let mut writers = Vec::new();
    for _ in 0..MAX_NESTING {
        writers.push(state.write().unwrap());
    }
    assert!(state.write().is_err());
    while let Some(writer) = writers.pop() {
        drop(writer);
    }
    let _writer = state.write().unwrap();
    for _ in 0..MAX_BINARIES {
        state.append(0, || Ok(Arc::new(Vec::new()))).unwrap();
    }
    assert!(state.append(0, || panic!("copy should not run")).is_err());
}

#[test]
fn internal_out_of_order_drop_fails_closed_instead_of_reusing_foreign_writer() {
    let state = Rc::new(State::default());
    let outer = state.write().unwrap();
    let inner = state.write().unwrap();
    drop(outer);
    assert!(state.poisoned.get());
    assert!(state.write().is_err());
    assert!(inner.finish("untrusted".into()).is_err());
}

#[test]
fn malformed_indices_never_consume_a_valid_message_slot() {
    let state = Rc::new(State::default());
    let writer = state.write().unwrap();
    let token = state.append(1, || Ok(bytes(1, 1))).unwrap();
    let _reader = state.read(writer.finish("graph".into()).unwrap()).unwrap();
    let prefix = token.rsplit_once('/').unwrap().0;
    for suffix in ["-1", "42949672960000000000000", "0/0", "garbage", "1"] {
        assert!(state.consume(&format!("{prefix}/{suffix}")).is_err());
    }
    assert_eq!(state.consume(&token).unwrap().len(), 1);
}
