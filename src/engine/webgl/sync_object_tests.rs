//! Fences use real native completion, task-stable status and bounded zero waits.
use super::api_version_tests::{call, version_two};
use super::*;
const COMPLETE: i64 = 0x9117;
const STATUS: i64 = 0x9114;
const UNSIGNALED: i64 = 0x9118;
const SIGNALED: i64 = 0x9119;
const TIMEOUT: i64 = 0x911b;
const ALREADY: i64 = 0x911a;

fn command(context: &mut WebGl, op: &str, values: &[i64]) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: values.into(),
            f: vec![],
            text: String::new(),
        },
        None,
    )
}
fn fence(context: &mut WebGl) -> i64 {
    call(context, "fenceSync", &[COMPLETE, 0], "")
        .as_i64()
        .unwrap()
}
fn status(context: &mut WebGl, id: i64) -> Value {
    call(context, "getSyncParameter", &[id, STATUS], "")
}

#[test]
fn webgl2_sync_completion_is_real_but_cannot_change_inside_a_task() {
    session::run_native_test(|| {
        let mut context = version_two();
        call(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
        let id = fence(&mut context);
        assert_eq!(call(&mut context, "isSync", &[id], ""), json!(true));
        call(&mut context, "finish", &[], "");
        for _ in 0..32 {
            assert_eq!(status(&mut context, id), json!(UNSIGNALED));
            assert_eq!(
                call(&mut context, "clientWaitSync", &[id, 0, 0], ""),
                json!(TIMEOUT)
            );
            assert_eq!(
                call(&mut context, "clientWaitSync", &[id, 1, 0], ""),
                json!(TIMEOUT)
            );
        }
        for _ in 0..100 {
            call(&mut context, "completeGpuTask", &[], "");
            if status(&mut context, id) == json!(SIGNALED) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(status(&mut context, id), json!(SIGNALED));
        for flags in [0, 1] {
            assert_eq!(
                call(&mut context, "clientWaitSync", &[id, flags, 0], ""),
                json!(ALREADY)
            );
        }
        let next = fence(&mut context);
        assert_ne!(id, next);
        assert_eq!(status(&mut context, next), json!(UNSIGNALED));
        assert_eq!(status(&mut context, id), json!(SIGNALED));
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_sync_fixed_properties_have_exact_native_reply_widths() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = fence(&mut context);
        for (pname, expected) in [(0x9112, 0x9116), (0x9113, COMPLETE), (0x9115, 0)] {
            assert_eq!(
                call(&mut context, "getSyncParameter", &[id, pname], ""),
                json!(expected)
            );
        }
        assert_eq!(call(&mut context, "getParameter", &[0x9247], ""), json!(0));
        assert_eq!(
            command(&mut context, "getSyncParameter", &[id, 0]),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(status(&mut context, id), json!(UNSIGNALED));
        call(&mut context, "waitSync", &[id, 0, -1], "");
        assert_eq!(status(&mut context, id), json!(UNSIGNALED));
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_sync_invalid_flags_timeouts_and_conditions_fail_before_driver_waits() {
    session::run_native_test(|| {
        let mut context = version_two();
        assert_eq!(
            command(&mut context, "fenceSync", &[0, 0]),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            command(&mut context, "fenceSync", &[COMPLETE, 1]),
            Err(gl::INVALID_VALUE)
        );
        let id = fence(&mut context);
        for (flags, timeout) in [(2, 0), (0, 1), (0, -1), (1, i64::MAX)] {
            assert_eq!(
                command(&mut context, "clientWaitSync", &[id, flags, timeout]),
                Err(gl::INVALID_OPERATION)
            );
        }
        for (flags, timeout) in [(1, -1), (0, 0), (0, i64::MAX), (0, -2)] {
            assert_eq!(
                command(&mut context, "waitSync", &[id, flags, timeout]),
                Err(gl::INVALID_VALUE)
            );
        }
        assert_eq!(status(&mut context, id), json!(UNSIGNALED));
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_sync_handles_never_alias_browser_objects_peer_contexts_or_deleted_fences() {
    session::run_native_test(|| {
        let mut peer = version_two();
        let foreign = fence(&mut peer);
        let mut context = version_two();
        let id = fence(&mut context);
        let buffer = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        let query = call(&mut context, "createQuery", &[], "").as_i64().unwrap();
        for invalid in [0, foreign, buffer, query, u32::MAX as i64] {
            assert_eq!(call(&mut context, "isSync", &[invalid], ""), json!(false));
            assert_eq!(
                command(&mut context, "clientWaitSync", &[invalid, 0, 0]),
                Err(gl::INVALID_OPERATION)
            );
            assert_eq!(
                command(&mut context, "getSyncParameter", &[invalid, STATUS]),
                Err(gl::INVALID_OPERATION)
            );
        }
        call(&mut context, "deleteSync", &[id], "");
        call(&mut context, "deleteSync", &[0], "");
        assert_eq!(call(&mut context, "isSync", &[id], ""), json!(false));
        assert_eq!(
            command(&mut context, "waitSync", &[id, 0, -1]),
            Err(gl::INVALID_OPERATION)
        );
        assert_ne!(fence(&mut context), id);
        let mut one = WebGl::new(1, 1, Options::default()).unwrap();
        assert_eq!(
            command(&mut one, "fenceSync", &[COMPLETE, 0]),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut one, "getParameter", &[0x9247]),
            Err(gl::INVALID_ENUM)
        );
    });
}

#[test]
fn webgl2_sync_limit_is_bounded_and_explicit_deletion_releases_live_slots() {
    session::run_native_test(|| {
        let mut context = version_two();
        let mut ids = Vec::new();
        for _ in 0..MAX_OBJECTS {
            ids.push(fence(&mut context));
        }
        assert_eq!(
            command(&mut context, "fenceSync", &[COMPLETE, 0]),
            Err(gl::OUT_OF_MEMORY)
        );
        call(&mut context, "deleteSync", &[ids[0]], "");
        let replacement = fence(&mut context);
        assert!(!ids.contains(&replacement));
        assert_eq!(
            call(&mut context, "isSync", &[replacement], ""),
            json!(true)
        );
        // Remaining native fences are destroyed with the owner context current.
    });
}
