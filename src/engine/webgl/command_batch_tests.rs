//! Real owner execution proves batching never postpones observable GL state.
use super::*;

fn create(contexts: &mut Contexts, api: &str) -> u32 {
    contexts
        .create(2, 2, &format!(r#"{{"api":"{api}"}}"#))
        .unwrap()
}

fn execute(contexts: &mut Contexts, id: u32, source: &str) -> Value {
    contexts.execute(id, source, None)
}

fn green(contexts: &mut Contexts, id: u32) {
    assert_eq!(
        execute(contexts, id, r#"{"op":"clearColor","f":[0,1,0,1]}"#),
        Value::Null
    );
    assert_eq!(
        execute(contexts, id, r#"{"op":"clear","i":[16384]}"#),
        Value::Null
    );
}

#[test]
fn observations_flush_earlier_setters_for_both_api_versions() {
    for api in ["webgl1", "webgl2"] {
        let mut contexts = Contexts::default();
        let id = create(&mut contexts, api);
        green(&mut contexts, id);
        assert_eq!(
            execute(&mut contexts, id, r#"{"op":"getParameter","i":[3106]}"#),
            json!([0., 1., 0., 1.])
        );
        assert_eq!(execute(&mut contexts, id, r#"{"op":"getError"}"#), json!(0));
        assert!(
            contexts
                .snapshot(id)
                .unwrap()
                .2
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
        green(&mut contexts, id);
        let PixelReply::Bytes(pixels) =
            contexts.read_pixels(id, r#"{"op":"readPixels","i":[0,0,1,1,6408,5121,4]}"#, None)
        else {
            panic!("binary read drains pending setters");
        };
        assert_eq!(pixels, [0, 255, 0, 255]);
        green(&mut contexts, id);
        contexts.complete_task();
        assert!(
            contexts
                .snapshot(id)
                .unwrap()
                .2
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
    }
}

#[test]
fn native_validation_and_errors_survive_bounded_automatic_flushes() {
    let mut contexts = Contexts::default();
    let id = create(&mut contexts, "webgl2");
    for index in 0..96 {
        assert_eq!(
            execute(&mut contexts, id, r#"{"op":"clearColor","f":[0,1,0,1]}"#),
            Value::Null
        );
        if index == 35 {
            execute(&mut contexts, id, r#"{"op":"enable","i":[57005]}"#);
        }
    }
    // A candidate still undergoes the complete native command parser.
    execute(
        &mut contexts,
        id,
        r#"{"op":"clear","i":[16384],"unknown":1}"#,
    );
    execute(&mut contexts, id, r#"{"op":"clear","op":"getError"}"#);
    assert_eq!(
        execute(&mut contexts, id, r#"{"op":"getError"}"#),
        json!(gl::INVALID_ENUM)
    );
    assert_eq!(
        execute(&mut contexts, id, r#"{"op":"getError"}"#),
        json!(gl::INVALID_VALUE)
    );
    assert_eq!(execute(&mut contexts, id, r#"{"op":"getError"}"#), json!(0));
    green(&mut contexts, id);
    assert!(
        contexts
            .snapshot(id)
            .unwrap()
            .2
            .chunks_exact(4)
            .all(|p| p == [0, 255, 0, 255])
    );
}

#[test]
fn uploaded_bytes_cannot_be_reordered_around_buffer_binding_or_deletion() {
    let mut contexts = Contexts::default();
    let id = create(&mut contexts, "webgl2");
    let first = execute(&mut contexts, id, r#"{"op":"createBuffer"}"#)
        .as_u64()
        .unwrap();
    let second = execute(&mut contexts, id, r#"{"op":"createBuffer"}"#)
        .as_u64()
        .unwrap();
    let bind = |buffer| json!({"op":"bindBuffer","i":[34962,buffer]}).to_string();
    let read = r#"{"op":"getBufferSubData","i":[34962,0,4]}"#;
    for (buffer, bytes) in [(first, [1, 2, 3, 4]), (second, [5, 6, 7, 8])] {
        execute(&mut contexts, id, &bind(buffer));
        contexts.execute(
            id,
            r#"{"op":"bufferData","i":[34962,4,35044]}"#,
            Some(&bytes),
        );
        let PixelReply::Bytes(actual) = contexts.read_pixels(id, read, None) else {
            panic!("read upload")
        };
        assert_eq!(actual, bytes);
    }
    execute(&mut contexts, id, &bind(first));
    let PixelReply::Bytes(actual) = contexts.read_pixels(id, read, None) else {
        panic!("read first")
    };
    assert_eq!(actual, [1, 2, 3, 4]);
    // Retirement must not cause a queued binding to escape its own context.
    contexts.remove(id);
    assert_eq!(
        execute(&mut contexts, id, &bind(first)),
        json!({"lost":true})
    );
}

#[test]
fn peer_contexts_and_task_stable_sync_queries_keep_their_own_order() {
    let mut contexts = Contexts::default();
    let first = create(&mut contexts, "webgl2");
    let second = create(&mut contexts, "webgl2");
    green(&mut contexts, first);
    execute(
        &mut contexts,
        second,
        r#"{"op":"clearColor","f":[1,0,0,1]}"#,
    );
    execute(&mut contexts, second, r#"{"op":"clear","i":[16384]}"#);
    let fence = execute(&mut contexts, first, r#"{"op":"fenceSync","i":[37143,0]}"#)
        .as_u64()
        .unwrap();
    let status = json!({"op":"getSyncParameter","i":[fence,0x9114]}).to_string();
    assert_eq!(execute(&mut contexts, first, &status), json!(0x9118));
    execute(&mut contexts, first, r#"{"op":"clearColor","f":[0,0,1,1]}"#);
    assert_eq!(execute(&mut contexts, first, &status), json!(0x9118));
    execute(&mut contexts, first, r#"{"op":"finish"}"#);
    contexts.complete_task();
    assert_eq!(execute(&mut contexts, first, &status), json!(0x9119));
    assert!(
        contexts
            .snapshot(first)
            .unwrap()
            .2
            .chunks_exact(4)
            .all(|p| p == [0, 255, 0, 255])
    );
    assert!(
        contexts
            .snapshot(second)
            .unwrap()
            .2
            .chunks_exact(4)
            .all(|p| p == [255, 0, 0, 255])
    );
}
