//! Uniform setters can share a submission without weakening native validation.
use super::*;

fn call(contexts: &mut Contexts, id: u32, op: &str, integers: &[i64], floats: &[f64]) -> Value {
    // The production bootstrap emits op first. This must exercise batching,
    // unlike serde_json::Map's differently ordered, synchronous encoding.
    let source = format!(
        r#"{{"op":"{op}","i":{},"f":{}}}"#,
        serde_json::to_string(integers).unwrap(),
        serde_json::to_string(floats).unwrap(),
    );
    contexts.execute(id, &source, None)
}

fn program(contexts: &mut Contexts, id: u32) -> i64 {
    let program = call(contexts, id, "createProgram", &[], &[])
        .as_i64()
        .unwrap();
    for (kind, source) in [
        (
            gl::VERTEX_SHADER,
            "attribute vec2 p;uniform mat4 m;void main(){gl_Position=m*vec4(p,0.,1.);}",
        ),
        (
            gl::FRAGMENT_SHADER,
            "precision mediump float;uniform vec4 c;uniform int n;void main(){gl_FragColor=c*float(n);}",
        ),
    ] {
        let shader = call(contexts, id, "createShader", &[kind as i64], &[])
            .as_i64()
            .unwrap();
        let command = json!({"op":"shaderSource","i":[shader],"text":source}).to_string();
        contexts.execute(id, &command, None);
        call(contexts, id, "compileShader", &[shader], &[]);
        call(contexts, id, "attachShader", &[program, shader], &[]);
    }
    // Binding the explicit attribute name is observable only through this text command.
    contexts.execute(
        id,
        &json!({"op":"bindAttribLocation","i":[program,0],"text":"p"}).to_string(),
        None,
    );
    call(contexts, id, "linkProgram", &[program], &[]);
    assert_eq!(
        call(
            contexts,
            id,
            "getProgramParameter",
            &[program, gl::LINK_STATUS as i64],
            &[]
        ),
        json!(true)
    );
    program
}

fn location(contexts: &mut Contexts, id: u32, program: i64, name: &str) -> i64 {
    contexts
        .execute(
            id,
            &json!({"op":"getUniformLocation","i":[program],"text":name}).to_string(),
            None,
        )
        .as_i64()
        .unwrap()
}

const IDENTITY: [f64; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

#[test]
fn matrix_and_vector_setters_flush_before_queries_and_native_draws() {
    for api in ["webgl1", "webgl2"] {
        let mut contexts = Contexts::default();
        let id = contexts
            .create(2, 2, &format!(r#"{{"api":"{api}"}}"#))
            .unwrap();
        let program = program(&mut contexts, id);
        let matrix = location(&mut contexts, id, program, "m");
        let color = location(&mut contexts, id, program, "c");
        let number = location(&mut contexts, id, program, "n");
        let buffer = call(&mut contexts, id, "createBuffer", &[], &[])
            .as_i64()
            .unwrap();
        call(
            &mut contexts,
            id,
            "bindBuffer",
            &[gl::ARRAY_BUFFER as i64, buffer],
            &[],
        );
        let bytes: Vec<_> = [-1.0_f32, -1., 3., -1., -1., 3.]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect();
        contexts.execute_owned(
            id,
            &json!({"op":"bufferData","i":[gl::ARRAY_BUFFER,bytes.len(),gl::STATIC_DRAW]})
                .to_string(),
            Some(bytes),
        );
        call(
            &mut contexts,
            id,
            "vertexAttribPointer",
            &[0, 2, gl::FLOAT as i64, 0, 0, 0],
            &[],
        );
        call(&mut contexts, id, "enableVertexAttribArray", &[0], &[]);
        call(&mut contexts, id, "useProgram", &[program], &[]);
        call(
            &mut contexts,
            id,
            "uniformMatrix4fv",
            &[matrix, 0],
            &IDENTITY,
        );
        call(&mut contexts, id, "uniform4fv", &[color], &[0., 1., 0., 1.]);
        call(&mut contexts, id, "uniform1iv", &[number, 1], &[]);
        assert_eq!(
            call(&mut contexts, id, "getUniform", &[program, color], &[])["values"],
            json!([0., 1., 0., 1.])
        );
        call(
            &mut contexts,
            id,
            "drawArrays",
            &[gl::TRIANGLES as i64, 0, 3],
            &[],
        );
        let bitmap = contexts.snapshot(id).unwrap();
        assert!(
            bitmap
                .2
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255])
        );
        assert_eq!(call(&mut contexts, id, "getError", &[], &[]), json!(0));
    }
}

#[test]
fn queued_uniforms_preserve_program_switches_and_automatic_flush_order() {
    let mut contexts = Contexts::default();
    let id = contexts.create(1, 1, "{}").unwrap();
    let first = program(&mut contexts, id);
    let second = program(&mut contexts, id);
    let first_color = location(&mut contexts, id, first, "c");
    let second_color = location(&mut contexts, id, second, "c");
    for index in 0..80 {
        call(&mut contexts, id, "useProgram", &[first], &[]);
        call(
            &mut contexts,
            id,
            "uniform4fv",
            &[first_color],
            &[index as f64, 0., 0., 1.],
        );
        call(&mut contexts, id, "useProgram", &[second], &[]);
        call(
            &mut contexts,
            id,
            "uniform4fv",
            &[second_color],
            &[0., index as f64, 0., 1.],
        );
    }
    assert_eq!(
        call(&mut contexts, id, "getUniform", &[first, first_color], &[])["values"],
        json!([79., 0., 0., 1.])
    );
    assert_eq!(
        call(
            &mut contexts,
            id,
            "getUniform",
            &[second, second_color],
            &[]
        )["values"],
        json!([0., 79., 0., 1.])
    );
    assert_eq!(call(&mut contexts, id, "getError", &[], &[]), json!(0));
}

#[test]
fn batching_keeps_shape_transpose_generation_and_foreign_location_errors() {
    let mut contexts = Contexts::default();
    let id = contexts.create(1, 1, "{}").unwrap();
    let first = program(&mut contexts, id);
    let second = program(&mut contexts, id);
    let matrix = location(&mut contexts, id, first, "m");
    let color = location(&mut contexts, id, first, "c");
    call(&mut contexts, id, "useProgram", &[first], &[]);
    call(&mut contexts, id, "uniform4fv", &[color], &[1., 2., 3.]);
    assert_eq!(
        call(&mut contexts, id, "getError", &[], &[]),
        json!(gl::INVALID_VALUE)
    );
    call(
        &mut contexts,
        id,
        "uniformMatrix4fv",
        &[matrix, 1],
        &IDENTITY,
    );
    assert_eq!(
        call(&mut contexts, id, "getError", &[], &[]),
        json!(gl::INVALID_VALUE)
    );
    call(&mut contexts, id, "useProgram", &[second], &[]);
    call(&mut contexts, id, "uniform4fv", &[color], &[0., 1., 0., 1.]);
    assert_eq!(
        call(&mut contexts, id, "getError", &[], &[]),
        json!(gl::INVALID_OPERATION)
    );
    call(&mut contexts, id, "linkProgram", &[first], &[]);
    call(&mut contexts, id, "useProgram", &[first], &[]);
    call(&mut contexts, id, "uniform4fv", &[color], &[0., 1., 0., 1.]);
    assert_eq!(
        call(&mut contexts, id, "getError", &[], &[]),
        json!(gl::INVALID_OPERATION)
    );
}
