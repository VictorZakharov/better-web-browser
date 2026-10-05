//! Real owner-thread publication without an author-controlled completion opcode.
use super::*;
use std::time::{Duration, Instant};

const COMPLETE: i64 = 0x9117;
const STATUS: i64 = 0x9114;
const UNSIGNALED: i64 = 0x9118;
const SIGNALED: i64 = 0x9119;
const ANY: i64 = 0x8c2f;
const AVAILABLE: i64 = 0x8867;
const RESULT: i64 = 0x8866;

fn command(contexts: &mut Contexts, id: u32, op: &str, values: &[i64]) -> Value {
    contexts.execute(id, &json!({"op":op,"i":values}).to_string(), None)
}
fn create(contexts: &mut Contexts) -> u32 {
    contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap()
}
fn fence(contexts: &mut Contexts, id: u32) -> i64 {
    command(contexts, id, "fenceSync", &[COMPLETE, 0])
        .as_i64()
        .unwrap()
}
fn status(contexts: &mut Contexts, id: u32, sync: i64) -> Value {
    command(contexts, id, "getSyncParameter", &[sync, STATUS])
}
fn publish_until(contexts: &mut Contexts, mut ready: impl FnMut(&mut Contexts) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        contexts.complete_task();
        if ready(contexts) {
            return;
        }
        assert!(Instant::now() < deadline, "native completion never arrived");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn webgl2_owner_task_boundary_publishes_all_and_only_its_realms_contexts() {
    let mut first = Contexts::default();
    let mut second = Contexts::default();
    let a = create(&mut first);
    let b = create(&mut first);
    let peer = create(&mut second);
    let sa = fence(&mut first, a);
    let sb = fence(&mut first, b);
    let sp = fence(&mut second, peer);
    for (contexts, id) in [(&mut first, a), (&mut second, peer)] {
        command(contexts, id, "finish", &[]);
    }
    command(&mut first, b, "finish", &[]);
    publish_until(&mut first, |contexts| {
        status(contexts, a, sa) == json!(SIGNALED) && status(contexts, b, sb) == json!(SIGNALED)
    });
    assert_eq!(status(&mut second, peer, sp), json!(UNSIGNALED));
    publish_until(&mut second, |contexts| {
        status(contexts, peer, sp) == json!(SIGNALED)
    });
    assert_eq!(command(&mut first, a, "getError", &[]), json!(0));
    assert_eq!(command(&mut first, b, "getError", &[]), json!(0));
    assert_eq!(command(&mut second, peer, "getError", &[]), json!(0));
}

#[test]
fn webgl2_author_commands_finish_and_presentation_cannot_publish_a_fence() {
    let mut contexts = Contexts::default();
    let id = create(&mut contexts);
    let sync = fence(&mut contexts, id);
    command(&mut contexts, id, "finish", &[]);
    assert!(contexts.snapshot(id).is_some());
    assert_eq!(status(&mut contexts, id, sync), json!(UNSIGNALED));
    assert_eq!(
        command(&mut contexts, id, "completeGpuTask", &[]),
        Value::Null
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[]),
        json!(gl::INVALID_OPERATION)
    );
    for _ in 0..8 {
        command(&mut contexts, id, "flush", &[]);
        assert_eq!(status(&mut contexts, id, sync), json!(UNSIGNALED));
        assert_eq!(
            command(&mut contexts, id, "clientWaitSync", &[sync, 1, 0]),
            json!(0x911b)
        );
    }
    publish_until(&mut contexts, |contexts| {
        status(contexts, id, sync) == json!(SIGNALED)
    });
    let next = fence(&mut contexts, id);
    command(&mut contexts, id, "finish", &[]);
    assert_eq!(status(&mut contexts, id, next), json!(UNSIGNALED));
    assert_eq!(status(&mut contexts, id, sync), json!(SIGNALED));
}

#[test]
fn webgl2_owner_task_boundary_publishes_real_zero_query_results() {
    let mut contexts = Contexts::default();
    let id = create(&mut contexts);
    let query = command(&mut contexts, id, "createQuery", &[])
        .as_i64()
        .unwrap();
    command(&mut contexts, id, "beginQuery", &[ANY, query]);
    command(&mut contexts, id, "endQuery", &[ANY]);
    command(&mut contexts, id, "finish", &[]);
    assert_eq!(
        command(&mut contexts, id, "getQueryParameter", &[query, AVAILABLE]),
        json!(false)
    );
    publish_until(&mut contexts, |contexts| {
        command(contexts, id, "getQueryParameter", &[query, AVAILABLE]) == json!(true)
    });
    assert_eq!(
        command(&mut contexts, id, "getQueryParameter", &[query, RESULT]),
        json!(0)
    );
    command(&mut contexts, id, "beginQuery", &[ANY, query]);
    contexts.complete_task();
    assert_eq!(command(&mut contexts, id, "getError", &[]), json!(0));
    command(&mut contexts, id, "endQuery", &[ANY]);
    assert_eq!(
        command(&mut contexts, id, "getQueryParameter", &[query, AVAILABLE]),
        json!(false)
    );
}

#[test]
fn webgl2_owner_task_boundary_preserves_sticky_errors_and_skips_webgl1() {
    let mut contexts = Contexts::default();
    let two = create(&mut contexts);
    let one = contexts.create(1, 1, "{}").unwrap();
    command(&mut contexts, two, "enable", &[0]);
    command(&mut contexts, one, "enable", &[0]);
    let sync = fence(&mut contexts, two);
    command(&mut contexts, two, "finish", &[]);
    publish_until(&mut contexts, |contexts| {
        status(contexts, two, sync) == json!(SIGNALED)
    });
    for id in [one, two] {
        assert_eq!(
            command(&mut contexts, id, "getError", &[]),
            json!(gl::INVALID_ENUM)
        );
        assert_eq!(command(&mut contexts, id, "getError", &[]), json!(0));
    }
    contexts.remove(two);
    contexts.complete_task();
    assert_eq!(
        command(&mut contexts, two, "getError", &[]),
        json!({"lost":true})
    );
    assert_eq!(command(&mut contexts, one, "getError", &[]), json!(0));
    contexts.clear();
    contexts.complete_task();
}

#[test]
fn webgl2_trusted_backend_completion_reports_missing_ids_without_poisoning_peers() {
    session::run_native_test(|| {
        let mut backend = BackendContexts::default();
        let id = backend.create(1, 1, r#"{"api":"webgl2"}"#).unwrap();
        assert_eq!(backend.complete_task(&[0, id, u32::MAX]), vec![0, u32::MAX]);
        assert_eq!(backend.execute(id, r#"{"op":"getError"}"#, None), json!(0));
        backend.remove(id);
        assert_eq!(backend.complete_task(&[id]), vec![id]);
        assert!(backend.complete_task(&[]).is_empty());
    });
}
