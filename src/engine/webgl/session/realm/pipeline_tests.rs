//! Actual ANGLE observations drain submitted and unsent commands in one order.
use super::*;
use crate::engine::webgl::NumericCommand;

fn context() -> (Contexts, u32) {
    let mut contexts = Contexts::default();
    let id = contexts
        .create(2, 2, r#"{"api":"webgl2","antialias":false}"#)
        .unwrap();
    (contexts, id)
}

fn numeric(contexts: &mut Contexts, id: u32, op: &str, i: &[i64], f: &[f64]) {
    assert!(contexts.execute_numeric(
        id,
        NumericCommand::new(op.into(), i.to_vec(), f.to_vec()).unwrap()
    ));
}

fn colors(contexts: &mut Contexts, id: u32, count: usize) {
    for _ in 0..count {
        numeric(contexts, id, "clearColor", &[], &[1., 0., 0., 1.]);
    }
}

fn error(contexts: &mut Contexts, id: u32) -> Value {
    contexts.execute(id, r#"{"op":"getError"}"#, None)
}

#[test]
fn native_queries_drain_both_batches_and_preserve_error_order() {
    let (mut contexts, id) = context();
    numeric(&mut contexts, id, "enable", &[0xdead], &[]);
    colors(&mut contexts, id, 127);
    assert!(
        contexts.inflight.is_some(),
        "full void batch should be submitted, not synchronously awaited"
    );
    numeric(&mut contexts, id, "clearColor", &[], &[0., 1., 0., 1.]);
    assert_eq!(error(&mut contexts, id), json!(gl::INVALID_ENUM));
    assert!(contexts.inflight.is_none() && contexts.pending.take().is_empty());
    assert_eq!(
        contexts.execute(id, r#"{"op":"getParameter","i":[3106]}"#, None),
        json!([0., 1., 0., 1.])
    );
    assert_eq!(error(&mut contexts, id), json!(0));
}

#[test]
fn native_snapshots_and_binary_reads_observe_the_last_queued_draw() {
    let (mut contexts, id) = context();
    colors(&mut contexts, id, 256);
    assert!(contexts.inflight.is_some());
    numeric(&mut contexts, id, "clearColor", &[], &[0., 1., 0., 1.]);
    numeric(
        &mut contexts,
        id,
        "clear",
        &[gl::COLOR_BUFFER_BIT as i64],
        &[],
    );
    let (_, _, pixels) = contexts.snapshot(id).unwrap();
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 255, 0, 255])
    );
    assert!(contexts.inflight.is_none());
    colors(&mut contexts, id, 128);
    numeric(
        &mut contexts,
        id,
        "clear",
        &[gl::COLOR_BUFFER_BIT as i64],
        &[],
    );
    let PixelReply::Bytes(pixels) =
        contexts.read_pixels(id, r#"{"op":"readPixels","i":[0,0,1,1,6408,5121,4]}"#, None)
    else {
        panic!("binary observation must drain the last red frame")
    };
    assert_eq!(pixels, [255, 0, 0, 255]);
    assert_eq!(error(&mut contexts, id), json!(0));
}

#[test]
fn native_upload_and_trusted_task_boundary_drain_earlier_bindings() {
    let (mut contexts, id) = context();
    let buffer = contexts
        .execute(id, r#"{"op":"createBuffer"}"#, None)
        .as_i64()
        .unwrap();
    colors(&mut contexts, id, 127);
    numeric(
        &mut contexts,
        id,
        "bindBuffer",
        &[gl::ARRAY_BUFFER as i64, buffer],
        &[],
    );
    assert!(contexts.inflight.is_some());
    contexts.execute_owned(
        id,
        r#"{"op":"bufferData","i":[34962,4,35044]}"#,
        Some(vec![7, 8, 9, 10]),
    );
    let PixelReply::Bytes(bytes) =
        contexts.read_pixels(id, r#"{"op":"getBufferSubData","i":[34962,0,4]}"#, None)
    else {
        panic!("ordered native buffer upload")
    };
    assert_eq!(bytes, [7, 8, 9, 10]);
    colors(&mut contexts, id, 256);
    numeric(&mut contexts, id, "clearColor", &[], &[0., 0., 1., 1.]);
    contexts.complete_task();
    assert!(contexts.inflight.is_none() && contexts.pending.take().is_empty());
    assert_eq!(
        contexts.execute(id, r#"{"op":"getParameter","i":[3106]}"#, None),
        json!([0., 0., 1., 1.])
    );
    assert_eq!(error(&mut contexts, id), json!(0));
}

#[test]
fn native_cross_context_batches_keep_each_frame_and_retirement_separate() {
    let (mut contexts, first) = context();
    let second = contexts
        .create(2, 2, r#"{"api":"webgl2","antialias":false}"#)
        .unwrap();
    for _ in 0..64 {
        numeric(&mut contexts, first, "clearColor", &[], &[1., 0., 0., 1.]);
        numeric(&mut contexts, second, "clearColor", &[], &[0., 1., 0., 1.]);
    }
    assert!(contexts.inflight.is_some());
    contexts.remove(first);
    numeric(
        &mut contexts,
        second,
        "clear",
        &[gl::COLOR_BUFFER_BIT as i64],
        &[],
    );
    let (_, _, pixels) = contexts.snapshot(second).unwrap();
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 255, 0, 255])
    );
    assert!(!contexts.live.contains(&first) && contexts.live.contains(&second));
    assert_eq!(error(&mut contexts, second), json!(0));
}
