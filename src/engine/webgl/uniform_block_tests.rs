//! Reflected std140 layout and indexed bindings drive actual GLES3 pixels.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::*;
const UNIFORM: i64 = 0x8a11;

fn shader(context: &mut WebGl) -> u32 {
    program(
        context,
        VERTEX,
        "#version 300 es\nprecision highp float;layout(std140) uniform Paint {vec4 tint;mat2 matrix;};out vec4 color;void main(){color=vec4(matrix*tint.xy,tint.zw);}",
    )
}
fn buffer(context: &mut WebGl, size: usize, bytes: Option<&[u8]>) -> u32 {
    let id = call(context, "createBuffer", &[], "").as_u64().unwrap() as u32;
    call(context, "bindBuffer", &[UNIFORM, id as i64], "");
    context
        .dispatch(
            &Command {
                op: "bufferData".into(),
                i: vec![UNIFORM, size as i64, gl::DYNAMIC_DRAW as i64],
                f: vec![],
                text: String::new(),
            },
            bytes,
        )
        .unwrap();
    id
}
fn block(context: &mut WebGl, program: u32) -> u32 {
    call(context, "getUniformBlockIndex", &[program as i64], "Paint")
        .as_u64()
        .unwrap() as u32
}
fn draw(context: &mut WebGl) -> Result<Value> {
    context.dispatch(
        &Command {
            op: "drawArrays".into(),
            i: vec![gl::TRIANGLES as i64, 0, 3],
            f: vec![],
            text: String::new(),
        },
        None,
    )
}
fn data() -> Vec<u8> {
    // std140 mat2 columns occupy 16-byte vectors, not tightly packed scalars.
    [1f32, 0., 0., 1., 1., 0., 0., 0., 0., 1., 0., 0.]
        .map(f32::to_ne_bytes)
        .concat()
}
fn property(context: &mut WebGl, program: u32, block: u32, pname: u32) -> Value {
    call(
        context,
        "getActiveUniformBlockParameter",
        &[program as i64, block as i64, pname as i64],
        "",
    )
}

#[test]
fn webgl2_uniform_block_reflection_explains_std140_offsets_and_matrix_strides() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = shader(&mut context);
        let index = block(&mut context, program);
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, 0x8a36],
                ""
            ),
            json!(1)
        );
        assert_eq!(
            call(
                &mut context,
                "getActiveUniformBlockName",
                &[program as i64, index as i64],
                ""
            ),
            json!("Paint")
        );
        assert_eq!(property(&mut context, program, index, 0x8a40), json!(48));
        assert_eq!(property(&mut context, program, index, 0x8a42), json!(2));
        assert_eq!(property(&mut context, program, index, 0x8a44), json!(false));
        assert_eq!(property(&mut context, program, index, 0x8a46), json!(true));
        let indices = call(
            &mut context,
            "getUniformIndices",
            &[program as i64],
            r#"["tint","matrix","absent"]"#,
        );
        assert_eq!(indices[2], json!(u32::MAX));
        let active = json!([indices[0], indices[1]]).to_string();
        for (pname, expected) in [
            (0x8a3b, json!([0, 16])),
            (0x8a3d, json!([0, 16])),
            (0x8a3e, json!([false, false])),
            (0x8a3a, json!([index, index])),
        ] {
            assert_eq!(
                call(
                    &mut context,
                    "getActiveUniforms",
                    &[program as i64, pname],
                    &active
                ),
                expected
            );
        }
        let vector = property(&mut context, program, index, 0x8a43);
        let mut actual: Vec<_> = vector
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        let mut expected = vec![indices[0].as_u64().unwrap(), indices[1].as_u64().unwrap()];
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
        assert_eq!(
            call(
                &mut context,
                "getUniformBlockIndex",
                &[program as i64],
                "Missing"
            ),
            json!(u32::MAX)
        );
    });
}

#[test]
fn webgl2_uniform_buffer_base_uploads_and_subupdates_change_actual_shader_output() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = shader(&mut context);
        let index = block(&mut context, program);
        let bytes = data();
        let id = buffer(&mut context, bytes.len(), Some(&bytes));
        call(
            &mut context,
            "uniformBlockBinding",
            &[program as i64, index as i64, 2],
            "",
        );
        call(&mut context, "bindBufferBase", &[UNIFORM, 2, id as i64], "");
        assert_eq!(property(&mut context, program, index, 0x8a3f), json!(2));
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8a28, 2], ""),
            json!(id)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8a2a, 2], ""),
            json!(48)
        );
        draw(&mut context).unwrap();
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        let green = [0f32, 1., 0., 1.].map(f32::to_ne_bytes).concat();
        context
            .dispatch(
                &Command {
                    op: "bufferSubData".into(),
                    i: vec![UNIFORM, 0],
                    f: vec![],
                    text: String::new(),
                },
                Some(&green),
            )
            .unwrap();
        draw(&mut context).unwrap();
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_uniform_buffer_range_uses_checked_aligned_offset_and_reports_exact_range() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = shader(&mut context);
        let index = block(&mut context, program);
        let alignment = call(&mut context, "getParameter", &[0x8a34], "")
            .as_u64()
            .unwrap() as usize;
        let mut bytes = vec![0u8; alignment + 48];
        bytes[alignment..].copy_from_slice(&data());
        let id = buffer(&mut context, bytes.len(), Some(&bytes));
        call(
            &mut context,
            "uniformBlockBinding",
            &[program as i64, index as i64, 1],
            "",
        );
        call(
            &mut context,
            "bindBufferRange",
            &[UNIFORM, 1, id as i64, alignment as i64, 48],
            "",
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8a29, 1], ""),
            json!(alignment)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8a2a, 1], ""),
            json!(48)
        );
        for (offset, size) in [(-1, 48), (alignment as i64, 0), (alignment as i64, 49)] {
            let command = Command {
                op: "bindBufferRange".into(),
                i: vec![UNIFORM, 1, id as i64, offset, size],
                f: vec![],
                text: String::new(),
            };
            assert_eq!(context.dispatch(&command, None), Err(gl::INVALID_VALUE));
        }
        if alignment > 1 {
            let command = Command {
                op: "bindBufferRange".into(),
                i: vec![UNIFORM, 1, id as i64, 1, 48],
                f: vec![],
                text: String::new(),
            };
            assert_eq!(context.dispatch(&command, None), Err(gl::INVALID_VALUE));
        }
        draw(&mut context).unwrap();
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
    });
}

#[test]
fn webgl2_indexed_uniform_deletion_does_not_clobber_a_different_generic_binding() {
    session::run_native_test(|| {
        let mut context = version_two();
        let retained = buffer(&mut context, 48, None);
        call(
            &mut context,
            "bindBufferBase",
            &[UNIFORM, 0, retained as i64],
            "",
        );
        call(
            &mut context,
            "bindBufferBase",
            &[UNIFORM, 1, retained as i64],
            "",
        );
        let generic = buffer(&mut context, 16, None);
        call(&mut context, "deleteBuffer", &[retained as i64], "");
        for index in [0, 1] {
            assert_eq!(
                call(&mut context, "getIndexedParameter", &[0x8a28, index], ""),
                Value::Null
            );
            assert_eq!(
                call(&mut context, "getIndexedParameter", &[0x8a2a, index], ""),
                json!(0)
            );
        }
        assert_eq!(
            call(&mut context, "getParameter", &[0x8a28], ""),
            json!(generic)
        );
        assert_eq!(
            call(
                &mut context,
                "getBufferParameter",
                &[UNIFORM, gl::BUFFER_SIZE as i64],
                ""
            ),
            json!(16)
        );
        assert!(context.objects.get(retained, Kind::Buffer).is_err());
    });
}

#[test]
fn webgl2_undersized_uniform_buffers_reject_draws_without_altering_pixels() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context);
        let small = buffer(&mut context, 16, None);
        call(
            &mut context,
            "bindBufferBase",
            &[UNIFORM, 0, small as i64],
            "",
        );
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
    });
}
