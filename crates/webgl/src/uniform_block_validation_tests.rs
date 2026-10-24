//! Malformed reflection/range requests never enlarge native output buffers.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::*;

fn invoke(context: &mut WebGl, op: &str, integers: &[i64], text: &str) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: integers.into(),
            f: vec![],
            text: text.into(),
        },
        None,
    )
}
#[test]
fn webgl2_uniform_block_invalid_indices_and_result_shapes_are_closed() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = program(
            &mut context,
            VERTEX,
            "#version 300 es\nprecision highp float;layout(std140) uniform Data {vec4 value;};out vec4 color;void main(){color=value;}",
        );
        for (op, integers, text, error) in [
            (
                "getActiveUniformBlockName",
                vec![program as i64, u32::MAX as i64],
                "",
                gl::INVALID_VALUE,
            ),
            (
                "getActiveUniformBlockParameter",
                vec![program as i64, 0, 0xdead],
                "",
                gl::INVALID_ENUM,
            ),
            (
                "getActiveUniforms",
                vec![program as i64, 0x8a3b],
                "[4294967295]",
                gl::INVALID_VALUE,
            ),
            (
                "getActiveUniforms",
                vec![program as i64, 0xdead],
                "[0]",
                gl::INVALID_ENUM,
            ),
            (
                "getUniformIndices",
                vec![program as i64],
                "[1]",
                gl::INVALID_VALUE,
            ),
            (
                "getUniformIndices",
                vec![program as i64],
                r#"["bad\u0000name"]"#,
                gl::INVALID_VALUE,
            ),
            (
                "uniformBlockBinding",
                vec![program as i64, 0, context.indexed_uniforms.0.len() as i64],
                "",
                gl::INVALID_VALUE,
            ),
        ] {
            assert_eq!(invoke(&mut context, op, &integers, text), Err(error));
        }
        assert_eq!(
            call(
                &mut context,
                "getActiveUniforms",
                &[program as i64, 0x8a3b],
                "[]"
            ),
            json!([])
        );
        assert_eq!(
            call(&mut context, "getUniformIndices", &[program as i64], "[]"),
            json!([])
        );
        let block = call(
            &mut context,
            "getUniformBlockIndex",
            &[program as i64],
            "Data",
        );
        assert_ne!(block, json!(u32::MAX));
        let mut peer = version_two();
        assert_eq!(
            invoke(&mut peer, "getUniformBlockIndex", &[program as i64], "Data"),
            Err(gl::INVALID_OPERATION)
        );
    });
}

#[test]
fn webgl2_uniform_block_layout_restrictions_are_native_shader_rules() {
    session::run_native_test(|| {
        let mut context = version_two();
        for layout in ["shared", "packed"] {
            let source = format!(
                "#version 300 es\nprecision highp float;layout({layout}) uniform Data {{vec4 value;}};out vec4 color;void main(){{color=value;}}"
            );
            let shader =
                super::api_version_tests::compile(&mut context, gl::FRAGMENT_SHADER, &source);
            assert!(
                !super::api_version_tests::compiled(&mut context, shader),
                "{layout}"
            );
        }
    });
}

#[test]
fn webgl2_uniform_base_size_tracks_reallocation_but_explicit_range_does_not() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = call(&mut context, "createBuffer", &[], "")
            .as_u64()
            .unwrap();
        call(&mut context, "bindBuffer", &[0x8a11, id as i64], "");
        call(
            &mut context,
            "bufferData",
            &[0x8a11, 256, gl::DYNAMIC_DRAW as i64],
            "",
        );
        call(&mut context, "bindBufferBase", &[0x8a11, 0, id as i64], "");
        call(
            &mut context,
            "bindBufferRange",
            &[0x8a11, 1, id as i64, 0, 64],
            "",
        );
        call(
            &mut context,
            "bufferData",
            &[0x8a11, 128, gl::DYNAMIC_DRAW as i64],
            "",
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8a2a, 0], ""),
            json!(128)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8a2a, 1], ""),
            json!(64)
        );
        call(&mut context, "bindBufferRange", &[0x8a11, 1, 0, -1, -1], "");
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8a28, 1], ""),
            Value::Null
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8a2a, 1], ""),
            json!(0)
        );
    });
}
