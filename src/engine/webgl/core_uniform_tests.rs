//! Real linked reflection and GPU pixels cover GLES3's additional uniform shapes.
use super::api_version_tests::{call, compile, compiled, link, version_two};
use super::*;

pub(super) fn program(context: &mut WebGl, vertex: &str, fragment: &str) -> u32 {
    let vertex = compile(context, gl::VERTEX_SHADER, vertex);
    let fragment = compile(context, gl::FRAGMENT_SHADER, fragment);
    for shader in [vertex, fragment] {
        assert!(
            compiled(context, shader),
            "{}",
            call(context, "getShaderInfoLog", &[shader as i64], "")
        );
    }
    let program = link(context, vertex, fragment);
    assert_eq!(
        call(
            context,
            "getProgramParameter",
            &[program as i64, gl::LINK_STATUS as i64],
            ""
        ),
        json!(true)
    );
    call(context, "useProgram", &[program as i64], "");
    program
}

fn location(context: &mut WebGl, program: u32, name: &str) -> u32 {
    call(context, "getUniformLocation", &[program as i64], name)
        .as_u64()
        .expect("active native uniform") as u32
}

fn values(context: &mut WebGl, program: u32, location: u32) -> Value {
    call(
        context,
        "getUniform",
        &[program as i64, location as i64],
        "",
    )["values"]
        .clone()
}

pub(super) const VERTEX: &str = "#version 300 es\nvoid main(){int i=gl_VertexID;vec2 p=vec2(float((i<<1)&2),float(i&2));gl_Position=vec4(p*2.0-1.0,0,1);}";

#[test]
fn webgl2_unsigned_uniform_vectors_roundtrip_without_signed_truncation() {
    session::run_native_test(|| {
        let mut context = version_two();
        for (index, kind) in ["uint", "uvec2", "uvec3", "uvec4"].into_iter().enumerate() {
            let expression = if index == 0 { "data" } else { "data.x" };
            let fragment = format!(
                "#version 300 es\nprecision highp float;uniform highp {kind} data;out vec4 color;void main(){{color=vec4(float({expression})/4294967295.0,0,0,1);}}"
            );
            let program = program(&mut context, VERTEX, &fragment);
            let location = location(&mut context, program, "data");
            let expected = [u32::MAX, 0x80000000, 17, 0];
            let mut arguments = vec![location as i64];
            arguments.extend(expected[..index + 1].iter().map(|value| *value as i64));
            call(
                &mut context,
                &format!("uniform{}uiv", index + 1),
                &arguments,
                "",
            );
            assert_eq!(
                values(&mut context, program, location),
                json!(&expected[..index + 1])
            );
            call(
                &mut context,
                "drawArrays",
                &[gl::TRIANGLES as i64, 0, 3],
                "",
            );
            assert!(
                context
                    .surface
                    .snapshot()
                    .unwrap()
                    .chunks_exact(4)
                    .all(|pixel| pixel == [255, 0, 0, 255])
            );
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}

#[test]
fn webgl2_all_nonsquare_matrices_roundtrip_native_column_major_storage() {
    session::run_native_test(|| {
        let mut context = version_two();
        let dimensions = ["2x3", "2x4", "3x2", "3x4", "4x2", "4x3"];
        let declarations = dimensions
            .iter()
            .enumerate()
            .map(|(index, shape)| format!("uniform mat{shape} m{index};"))
            .collect::<String>();
        let sum = (0..6)
            .map(|index| format!("m{index}[0][0]"))
            .collect::<Vec<_>>()
            .join("+");
        let vertex = format!(
            "#version 300 es\n{declarations}void main(){{gl_Position=vec4(({sum})/1000.0,0,0,1);}}"
        );
        let program = program(
            &mut context,
            &vertex,
            "#version 300 es\nprecision mediump float;out vec4 color;void main(){color=vec4(1);}",
        );
        for (index, shape) in dimensions.into_iter().enumerate() {
            let location = location(&mut context, program, &format!("m{index}"));
            let expected = (0..core_uniforms::MATRIX_COMPONENTS[index])
                .map(|value| value as f64 + 0.5)
                .collect::<Vec<_>>();
            let command = Command {
                op: format!("uniformMatrix{shape}fv"),
                i: vec![location as i64, 0],
                f: expected.clone(),
                text: String::new(),
            };
            assert_eq!(context.dispatch(&command, None), Ok(Value::Null));
            assert_eq!(values(&mut context, program, location), json!(expected));
            let kind = context
                .objects
                .get(location, Kind::Uniform)
                .unwrap()
                .uniform_type;
            assert_eq!(kind, core_uniforms::MATRIX_TYPES[index]);
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}

#[test]
fn webgl2_uniform_validation_is_atomic_and_does_not_accept_stale_locations() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = program(
            &mut context,
            VERTEX,
            "#version 300 es\nprecision highp float;uniform highp uvec3 data;out vec4 color;void main(){color=vec4(vec3(data)/255.0,1);}",
        );
        let location = location(&mut context, program, "data");
        call(&mut context, "uniform3ui", &[location as i64, 1, 2, 3], "");
        for arguments in [
            vec![location as i64, 1, 2],
            vec![location as i64, -1, 2, 3],
            vec![location as i64, u32::MAX as i64 + 1, 2, 3],
        ] {
            let command = Command {
                op: "uniform3uiv".into(),
                i: arguments,
                f: vec![],
                text: String::new(),
            };
            assert_eq!(context.dispatch(&command, None), Err(gl::INVALID_VALUE));
            assert_eq!(values(&mut context, program, location), json!([1, 2, 3]));
        }
        let wrong_type = Command {
            op: "uniform2ui".into(),
            i: vec![location as i64, 5, 6],
            f: vec![],
            text: String::new(),
        };
        assert_eq!(
            context.dispatch(&wrong_type, None),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(values(&mut context, program, location), json!([1, 2, 3]));
        assert_eq!(
            call(&mut context, "getError", &[], ""),
            json!(gl::INVALID_OPERATION)
        );
        call(&mut context, "linkProgram", &[program as i64], "");
        let stale = Command {
            op: "uniform3ui".into(),
            i: vec![location as i64, 5, 6, 7],
            f: vec![],
            text: String::new(),
        };
        assert_eq!(context.dispatch(&stale, None), Err(gl::INVALID_OPERATION));
        // A null location is a no-op, regardless of the currently linked program.
        assert_eq!(
            call(&mut context, "uniform3ui", &[0, 0, 0, 0], ""),
            Value::Null
        );
    });
}

#[test]
fn webgl2_nonsquare_transpose_and_partial_elements_preserve_native_values() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = program(
            &mut context,
            "#version 300 es\nuniform mat3x2 m;void main(){gl_Position=vec4(m[0],0,1);}",
            "#version 300 es\nprecision mediump float;out vec4 color;void main(){color=vec4(1);}",
        );
        let location = location(&mut context, program, "m");
        for (transpose, count) in [(1, 6), (0, 5), (0, 7)] {
            let command = Command {
                op: "uniformMatrix3x2fv".into(),
                i: vec![location as i64, transpose],
                f: vec![7.; count],
                text: String::new(),
            };
            assert_eq!(context.dispatch(&command, None), Err(gl::INVALID_VALUE));
            assert_eq!(values(&mut context, program, location), json!(vec![0.; 6]));
        }
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl1_rejects_core_unsigned_and_nonsquare_operations_even_for_null_locations() {
    let (mut contexts, id) = tests::context();
    for operation in ["uniform1ui", "uniform2uiv", "uniformMatrix4x3fv"] {
        tests::command(&mut contexts, id, operation, &[0, 0], &[], "", None);
        assert_eq!(
            tests::command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_OPERATION)
        );
    }
}
