//! Native visibility results and task-stable asynchronous query publication.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::*;

const ANY: i64 = 0x8c2f;
const CONSERVATIVE: i64 = 0x8d6a;
const PRIMITIVES: i64 = 0x8c88;
const CURRENT: i64 = 0x8865;
const RESULT: i64 = 0x8866;
const AVAILABLE: i64 = 0x8867;

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
fn create(context: &mut WebGl) -> i64 {
    call(context, "createQuery", &[], "").as_i64().unwrap()
}
fn result(context: &mut WebGl, query: i64) -> Value {
    call(context, "getQueryParameter", &[query, RESULT], "")
}
fn ready(context: &mut WebGl, query: i64) -> Value {
    call(context, "getQueryParameter", &[query, AVAILABLE], "")
}
fn publish(context: &mut WebGl, query: i64) {
    // Test-only simulated host task boundaries. No public JavaScript method
    // exposes the internal boundary, and glFinish alone cannot publish results.
    for _ in 0..100 {
        context.complete_gpu_task().unwrap();
        if ready(context, query) == json!(true) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("native query never became ready");
}

#[test]
fn webgl2_query_visibility_uses_native_gpu_results_after_a_task_boundary() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(
            &mut context,
            VERTEX,
            "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1,0,0,1);}",
        );
        let query = create(&mut context);
        call(&mut context, "beginQuery", &[ANY, query], "");
        call(
            &mut context,
            "drawArrays",
            &[gl::TRIANGLES as i64, 0, 3],
            "",
        );
        call(&mut context, "endQuery", &[ANY], "");
        call(&mut context, "finish", &[], "");
        for _ in 0..16 {
            assert_eq!(ready(&mut context, query), json!(false));
            assert_eq!(result(&mut context, query), json!(0));
        }
        publish(&mut context, query);
        assert_eq!(result(&mut context, query), json!(1));
        call(&mut context, "beginQuery", &[ANY, query], "");
        call(&mut context, "enable", &[gl::SCISSOR_TEST as i64], "");
        call(&mut context, "scissor", &[0, 0, 0, 0], "");
        call(
            &mut context,
            "drawArrays",
            &[gl::TRIANGLES as i64, 0, 3],
            "",
        );
        call(&mut context, "endQuery", &[ANY], "");
        call(&mut context, "finish", &[], "");
        assert_eq!(ready(&mut context, query), json!(false));
        assert_eq!(result(&mut context, query), json!(0));
        publish(&mut context, query);
        assert_eq!(result(&mut context, query), json!(0));
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_query_targets_share_only_the_occlusion_slot_and_keep_exact_identity() {
    session::run_native_test(|| {
        let mut context = version_two();
        let query = create(&mut context);
        let other = create(&mut context);
        let primitive = create(&mut context);
        assert_eq!(call(&mut context, "isQuery", &[query], ""), json!(false));
        assert_eq!(
            command(&mut context, "getQueryParameter", &[query, RESULT]),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "beginQuery", &[ANY, query], "");
        assert_eq!(call(&mut context, "isQuery", &[query], ""), json!(true));
        assert_eq!(
            call(&mut context, "getQuery", &[ANY, CURRENT], ""),
            json!(query)
        );
        assert_eq!(
            call(&mut context, "getQuery", &[CONSERVATIVE, CURRENT], ""),
            Value::Null
        );
        assert_eq!(
            command(&mut context, "beginQuery", &[CONSERVATIVE, other]),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut context, "endQuery", &[CONSERVATIVE]),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut context, "getQueryParameter", &[query, AVAILABLE]),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "beginQuery", &[PRIMITIVES, primitive], "");
        assert_eq!(
            call(&mut context, "getQuery", &[PRIMITIVES, CURRENT], ""),
            json!(primitive)
        );
        call(&mut context, "endQuery", &[PRIMITIVES], "");
        call(&mut context, "endQuery", &[ANY], "");
        assert_eq!(
            command(&mut context, "beginQuery", &[CONSERVATIVE, query]),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "beginQuery", &[CONSERVATIVE, other], "");
        call(&mut context, "endQuery", &[CONSERVATIVE], "");
        assert_eq!(
            call(&mut context, "getQuery", &[ANY, CURRENT], ""),
            Value::Null
        );
        assert_eq!(
            command(&mut context, "endQuery", &[ANY]),
            Err(gl::INVALID_OPERATION)
        );
    });
}

#[test]
fn webgl2_query_deletion_implicitly_ends_only_its_own_target() {
    session::run_native_test(|| {
        let mut context = version_two();
        let query = create(&mut context);
        let primitive = create(&mut context);
        call(&mut context, "beginQuery", &[ANY, query], "");
        call(&mut context, "beginQuery", &[PRIMITIVES, primitive], "");
        call(&mut context, "deleteQuery", &[query], "");
        assert_eq!(
            call(&mut context, "getQuery", &[ANY, CURRENT], ""),
            Value::Null
        );
        assert_eq!(
            call(&mut context, "getQuery", &[PRIMITIVES, CURRENT], ""),
            json!(primitive)
        );
        assert_eq!(call(&mut context, "isQuery", &[query], ""), json!(false));
        let replacement = create(&mut context);
        assert_ne!(replacement, query);
        call(&mut context, "beginQuery", &[ANY, replacement], "");
        call(&mut context, "deleteQuery", &[replacement], "");
        call(&mut context, "endQuery", &[PRIMITIVES], "");
        call(&mut context, "deleteQuery", &[primitive], "");
        call(&mut context, "deleteQuery", &[0], "");
        assert_eq!(
            command(&mut context, "beginQuery", &[ANY, query]),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_queries_reject_peer_handles_enums_and_webgl1_without_state_changes() {
    session::run_native_test(|| {
        let mut peer = version_two();
        let foreign = create(&mut peer);
        let mut context = version_two();
        let query = create(&mut context);
        let buffer = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        for invalid in [foreign, buffer, 0, u32::MAX as i64] {
            assert_eq!(
                command(&mut context, "beginQuery", &[ANY, invalid]),
                Err(gl::INVALID_OPERATION)
            );
            assert_eq!(call(&mut context, "isQuery", &[invalid], ""), json!(false));
        }
        assert_eq!(
            command(&mut context, "beginQuery", &[0, query]),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            command(&mut context, "getQuery", &[ANY, 0]),
            Err(gl::INVALID_ENUM)
        );
        call(&mut context, "beginQuery", &[ANY, query], "");
        call(&mut context, "endQuery", &[ANY], "");
        assert_eq!(
            command(&mut context, "getQueryParameter", &[query, 0]),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(ready(&mut context, query), json!(false));
        assert_eq!(
            call(&mut context, "getQuery", &[ANY, CURRENT], ""),
            Value::Null
        );
        let mut one = WebGl::new(1, 1, Options::default()).unwrap();
        assert_eq!(
            command(&mut one, "createQuery", &[]),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut one, "beginQuery", &[ANY, query]),
            Err(gl::INVALID_OPERATION)
        );
    });
}
