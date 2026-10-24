//! Bypass author bindings: native metadata must determine writable query size.
use super::tests::{command, context};
use super::*;

fn call(contexts: &mut Contexts, id: u32, op: &str, values: &[u32]) -> Value {
    command(contexts, id, op, values, &[], "", None)
}

fn program(contexts: &mut Contexts, id: u32) -> u32 {
    let program = call(contexts, id, "createProgram", &[]).as_u64().unwrap() as u32;
    for (kind, source) in [
        (gl::VERTEX_SHADER, "void main(){gl_Position=vec4(0.);}"),
        (
            gl::FRAGMENT_SHADER,
            "precision mediump float; uniform vec2 pair;void main(){gl_FragColor=vec4(pair,0.,1.);}",
        ),
    ] {
        let shader = call(contexts, id, "createShader", &[kind])
            .as_u64()
            .unwrap() as u32;
        command(contexts, id, "shaderSource", &[shader], &[], source, None);
        call(contexts, id, "compileShader", &[shader]);
        assert_eq!(
            call(
                contexts,
                id,
                "getShaderParameter",
                &[shader, gl::COMPILE_STATUS]
            ),
            json!(true)
        );
        call(contexts, id, "attachShader", &[program, shader]);
    }
    call(contexts, id, "linkProgram", &[program]);
    assert_eq!(
        call(
            contexts,
            id,
            "getProgramParameter",
            &[program, gl::LINK_STATUS]
        ),
        json!(true)
    );
    program
}

fn location(contexts: &mut Contexts, id: u32, program: u32) -> u32 {
    command(
        contexts,
        id,
        "getUniformLocation",
        &[program],
        &[],
        "pair",
        None,
    )
    .as_u64()
    .unwrap() as u32
}

#[test]
fn native_uniform_query_ignores_forged_author_type_and_component_count() {
    let (mut contexts, id) = context();
    let program = program(&mut contexts, id);
    let location = location(&mut contexts, id, program);
    call(&mut contexts, id, "useProgram", &[program]);
    command(
        &mut contexts,
        id,
        "uniform2f",
        &[location],
        &[0.25, 0.75],
        "",
        None,
    );
    for forged_kind in [gl::FLOAT_MAT4, gl::INT, gl::BOOL_VEC4, u32::MAX] {
        let actual = command(
            &mut contexts,
            id,
            "getUniform",
            &[program, location, forged_kind, u32::MAX],
            &[],
            "invented author reflection",
            None,
        );
        assert_eq!(actual["kind"], json!(gl::FLOAT_VEC2));
        assert_eq!(actual["values"], json!([0.25, 0.75]));
        assert_eq!(
            call(&mut contexts, id, "getError", &[]),
            json!(gl::NO_ERROR)
        );
    }
    let payload = json!({"op":"getUniform", "i":[program,location], "kind":gl::FLOAT_MAT4});
    assert_eq!(
        contexts.execute(id, &payload.to_string(), None),
        Value::Null
    );
    assert_eq!(
        call(&mut contexts, id, "getError", &[]),
        json!(gl::INVALID_VALUE)
    );
    assert_eq!(
        call(&mut contexts, id, "getUniform", &[program, location])["values"],
        json!([0.25, 0.75])
    );
}

#[test]
fn native_uniform_queries_reject_wrong_resource_kinds_owners_and_stale_generations() {
    let (mut contexts, id) = context();
    let first = program(&mut contexts, id);
    let second = program(&mut contexts, id);
    let uniform = location(&mut contexts, id, first);
    let buffer = call(&mut contexts, id, "createBuffer", &[])
        .as_u64()
        .unwrap() as u32;
    for args in [
        [first, buffer],
        [buffer, uniform],
        [second, uniform],
        [first, u32::MAX],
        [u32::MAX, uniform],
    ] {
        assert_eq!(call(&mut contexts, id, "getUniform", &args), Value::Null);
        assert_eq!(
            call(&mut contexts, id, "getError", &[]),
            json!(gl::INVALID_OPERATION)
        );
    }
    call(&mut contexts, id, "linkProgram", &[first]);
    assert_eq!(
        call(&mut contexts, id, "getUniform", &[first, uniform]),
        Value::Null
    );
    assert_eq!(
        call(&mut contexts, id, "getError", &[]),
        json!(gl::INVALID_OPERATION)
    );
    let fresh = location(&mut contexts, id, first);
    assert_ne!(fresh, uniform);
    assert_eq!(
        call(&mut contexts, id, "getUniform", &[first, fresh])["values"],
        json!([0.0, 0.0])
    );
    assert_eq!(
        call(&mut contexts, id, "getError", &[]),
        json!(gl::NO_ERROR)
    );
}

#[test]
fn native_uniform_query_checks_all_required_indices_without_touching_driver_storage() {
    let (mut contexts, id) = context();
    let program = program(&mut contexts, id);
    let uniform = location(&mut contexts, id, program);
    let before = contexts.snapshot(id).unwrap().2;
    for args in [vec![], vec![program], vec![program, 0]] {
        assert_eq!(call(&mut contexts, id, "getUniform", &args), Value::Null);
        let error = call(&mut contexts, id, "getError", &[]);
        assert!(error == json!(gl::INVALID_VALUE) || error == json!(gl::INVALID_OPERATION));
    }
    assert_eq!(contexts.snapshot(id).unwrap().2, before);
    assert_eq!(
        call(&mut contexts, id, "getUniform", &[program, uniform])["kind"],
        json!(gl::FLOAT_VEC2)
    );
    assert_eq!(
        call(&mut contexts, id, "getError", &[]),
        json!(gl::NO_ERROR)
    );
}
