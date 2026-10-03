//! Real GPU vertex output, not a CPU imitation of transform feedback.
use super::api_version_tests::{call, compile, compiled, link, version_two};
use super::*;
pub(super) const TARGET: i64 = 0x8c8e;
pub(super) const OBJECT: i64 = 0x8e22;
pub(super) const INTERLEAVED: i64 = 0x8c8c;
pub(super) const SEPARATE: i64 = 0x8c8d;

pub(super) fn command(context: &mut WebGl, op: &str, values: &[i64], text: &str) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: values.into(),
            f: vec![],
            text: text.into(),
        },
        None,
    )
}
pub(super) fn shader(context: &mut WebGl, mode: i64, names: &str) -> u32 {
    let vertex = compile(
        context,
        gl::VERTEX_SHADER,
        "#version 300 es\nout vec2 captured;flat out uint identity;void main(){captured=vec2(float(gl_VertexID)+0.5,-float(gl_VertexID));identity=uint(gl_VertexID)+2147483648u;gl_Position=vec4(0,0,0,1);gl_PointSize=1.0;}",
    );
    let fragment = compile(
        context,
        gl::FRAGMENT_SHADER,
        "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1);}",
    );
    assert!(compiled(context, vertex));
    assert!(compiled(context, fragment));
    let program = call(context, "createProgram", &[], "").as_u64().unwrap() as u32;
    call(
        context,
        "attachShader",
        &[program as i64, vertex as i64],
        "",
    );
    call(
        context,
        "attachShader",
        &[program as i64, fragment as i64],
        "",
    );
    call(
        context,
        "transformFeedbackVaryings",
        &[program as i64, mode],
        names,
    );
    call(context, "linkProgram", &[program as i64], "");
    assert_eq!(
        call(
            context,
            "getProgramParameter",
            &[program as i64, gl::LINK_STATUS as i64],
            ""
        ),
        json!(true),
        "{}",
        call(context, "getProgramInfoLog", &[program as i64], "")
    );
    call(context, "useProgram", &[program as i64], "");
    program
}
pub(super) fn buffer(context: &mut WebGl, size: usize) -> i64 {
    let id = call(context, "createBuffer", &[], "").as_i64().unwrap();
    call(context, "bindBuffer", &[TARGET, id], "");
    call(context, "bufferData", &[TARGET, size as i64, 0x88e1], "");
    id
}
pub(super) fn object(context: &mut WebGl) -> i64 {
    call(context, "createTransformFeedback", &[], "")
        .as_i64()
        .unwrap()
}
pub(super) fn read(context: &mut WebGl, id: i64, size: usize) -> Vec<u8> {
    call(context, "bindBuffer", &[TARGET, id], "");
    context
        .read_buffer(&Command {
            op: "getBufferSubData".into(),
            i: vec![TARGET, 0, size as i64],
            f: vec![],
            text: String::new(),
        })
        .unwrap()
}
pub(super) fn draw(context: &mut WebGl, start: i64, count: i64) {
    call(
        context,
        "drawArrays",
        &[gl::POINTS as i64, start, count],
        "",
    );
}

#[test]
fn webgl2_transform_interleaved_outputs_preserve_float_and_full_uint_values() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context, INTERLEAVED, r#"["captured","identity"]"#);
        let id = buffer(&mut context, 36);
        call(&mut context, "bindBufferBase", &[TARGET, 0, id], "");
        call(&mut context, "enable", &[0x8c89], "");
        let query = call(&mut context, "createQuery", &[], "").as_i64().unwrap();
        call(&mut context, "beginQuery", &[0x8c88, query], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        draw(&mut context, 0, 3);
        call(&mut context, "endTransformFeedback", &[], "");
        call(&mut context, "endQuery", &[0x8c88], "");
        let data = read(&mut context, id, 36);
        for (index, vertex) in data.chunks_exact(12).enumerate() {
            assert_eq!(
                f32::from_ne_bytes(vertex[..4].try_into().unwrap()),
                index as f32 + 0.5
            );
            assert_eq!(
                f32::from_ne_bytes(vertex[4..8].try_into().unwrap()),
                -(index as f32)
            );
            assert_eq!(
                u32::from_ne_bytes(vertex[8..12].try_into().unwrap()),
                0x8000_0000 + index as u32
            );
        }
        call(&mut context, "finish", &[], "");
        call(&mut context, "completeGpuTask", &[], "");
        assert_eq!(
            call(&mut context, "getQueryParameter", &[query, 0x8867], ""),
            json!(true)
        );
        assert_eq!(
            call(&mut context, "getQueryParameter", &[query, 0x8866], ""),
            json!(3)
        );
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .iter()
                .all(|&byte| byte == 0)
        );
    });
}

#[test]
fn webgl2_transform_separate_ranges_preserve_unwritten_prefix_and_suffix() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context, SEPARATE, r#"["captured","identity"]"#);
        let vectors = buffer(&mut context, 40);
        let integers = buffer(&mut context, 24);
        call(
            &mut context,
            "bindBufferRange",
            &[TARGET, 0, vectors, 8, 24],
            "",
        );
        call(
            &mut context,
            "bindBufferRange",
            &[TARGET, 1, integers, 4, 12],
            "",
        );
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        draw(&mut context, 0, 3);
        call(&mut context, "endTransformFeedback", &[], "");
        let data = read(&mut context, vectors, 40);
        assert!(data[..8].iter().chain(&data[32..]).all(|&byte| byte == 0));
        for (index, vector) in data[8..32].chunks_exact(8).enumerate() {
            assert_eq!(
                f32::from_ne_bytes(vector[..4].try_into().unwrap()),
                index as f32 + 0.5
            );
        }
        let data = read(&mut context, integers, 24);
        assert!(data[..4].iter().chain(&data[16..]).all(|&byte| byte == 0));
        for (index, integer) in data[4..16].chunks_exact(4).enumerate() {
            assert_eq!(
                u32::from_ne_bytes(integer.try_into().unwrap()),
                0x8000_0000 + index as u32
            );
        }
    });
}

#[test]
fn webgl2_transform_pause_resume_skips_paused_draws_without_resetting_capture_offset() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context, INTERLEAVED, r#"["captured"]"#);
        let id = buffer(&mut context, 16);
        call(&mut context, "bindBufferBase", &[TARGET, 0, id], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        draw(&mut context, 0, 1);
        call(&mut context, "pauseTransformFeedback", &[], "");
        draw(&mut context, 1, 1);
        assert_eq!(
            call(&mut context, "getParameter", &[0x8e23], ""),
            json!(true)
        );
        call(&mut context, "resumeTransformFeedback", &[], "");
        draw(&mut context, 2, 1);
        call(&mut context, "endTransformFeedback", &[], "");
        let data = read(&mut context, id, 16);
        let values = data
            .chunks_exact(4)
            .map(|bytes| f32::from_ne_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(values, [0.5, 0., 2.5, -2.]);
        assert_eq!(
            call(&mut context, "getParameter", &[0x8e24], ""),
            json!(false)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8e23], ""),
            json!(false)
        );
    });
}

#[test]
fn webgl2_transform_reflection_reports_linked_varying_names_types_and_mode() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = shader(&mut context, SEPARATE, r#"["captured","identity"]"#);
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, 0x8c83],
                ""
            ),
            json!(2)
        );
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, 0x8c7f],
                ""
            ),
            json!(SEPARATE)
        );
        assert_eq!(
            call(
                &mut context,
                "getTransformFeedbackVarying",
                &[program as i64, 0],
                ""
            ),
            json!({"name":"captured","size":1,"type":gl::FLOAT_VEC2})
        );
        assert_eq!(
            call(
                &mut context,
                "getTransformFeedbackVarying",
                &[program as i64, 1],
                ""
            ),
            json!({"name":"identity","size":1,"type":gl::UNSIGNED_INT})
        );
        assert_eq!(
            command(
                &mut context,
                "getTransformFeedbackVarying",
                &[program as i64, 2],
                ""
            ),
            Err(gl::INVALID_VALUE)
        );
        // Changing the next-link list cannot retroactively change linked output.
        call(
            &mut context,
            "transformFeedbackVaryings",
            &[program as i64, INTERLEAVED],
            r#"["identity"]"#,
        );
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, 0x8c83],
                ""
            ),
            json!(2)
        );
    });
}

#[test]
fn webgl2_transform_provider_rejects_unsupported_array_capture_without_a_fake_result() {
    session::run_native_test(|| {
        let mut context = version_two();
        let vertex = compile(
            &mut context,
            gl::VERTEX_SHADER,
            "#version 300 es\nout float values[2];void main(){values[0]=3.0;values[1]=7.0;gl_Position=vec4(0,0,0,1);}",
        );
        let fragment = compile(
            &mut context,
            gl::FRAGMENT_SHADER,
            "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(0);}",
        );
        let program = link(&mut context, vertex, fragment);
        call(
            &mut context,
            "transformFeedbackVaryings",
            &[program as i64, INTERLEAVED],
            r#"["values"]"#,
        );
        call(&mut context, "linkProgram", &[program as i64], "");
        // The pinned exact GLES3.0 provider permits arrays in shaders but does
        // not stream them. Keep this limitation visible before realm admission.
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, gl::LINK_STATUS as i64],
                ""
            ),
            json!(false)
        );
        let log = call(&mut context, "getProgramInfoLog", &[program as i64], "");
        assert!(log.as_str().unwrap().contains("Capture of arrays"), "{log}");
        assert_eq!(
            command(&mut context, "useProgram", &[program as i64], ""),
            Err(gl::INVALID_OPERATION)
        );
    });
}
