//! Declaring ranges before bufferData is legal; GPU use still checks real storage.
use super::api_version_tests::{call, version_two};
use super::transform_feedback_tests::command;
use super::*;

#[test]
fn webgl2_instanced_point_capture_rejects_64_bit_capacity_overflow_before_work_budget() {
    session::run_native_test(|| {
        use super::transform_feedback_tests::{INTERLEAVED, TARGET, buffer, read, shader};
        let mut context = version_two();
        shader(&mut context, INTERLEAVED, r#"["captured"]"#);
        let output = buffer(&mut context, 32);
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        for (count, instances) in [(65536, 65536), (i32::MAX as i64, i32::MAX as i64)] {
            assert_eq!(
                command(
                    &mut context,
                    "drawArraysInstanced",
                    &[gl::POINTS as i64, 0, count, instances],
                    ""
                ),
                Err(gl::INVALID_OPERATION)
            );
        }
        call(&mut context, "pauseTransformFeedback", &[], "");
        assert_eq!(
            command(
                &mut context,
                "drawArraysInstanced",
                &[gl::POINTS as i64, 0, 65536, 65536],
                ""
            ),
            Err(gl::INVALID_VALUE)
        );
        call(&mut context, "endTransformFeedback", &[], "");
        assert!(read(&mut context, output, 32).iter().all(|byte| *byte == 0));
    });
}

#[test]
fn webgl2_first_copy_binding_establishes_other_data_without_reclassifying_indices() {
    session::run_native_test(|| {
        let mut context = version_two();
        for copy in [
            super::core_buffers::COPY_READ,
            super::core_buffers::COPY_WRITE,
        ] {
            let other = call(&mut context, "createBuffer", &[], "")
                .as_i64()
                .unwrap();
            call(&mut context, "bindBuffer", &[copy as i64, other], "");
            assert_eq!(
                context
                    .objects
                    .get(other as u32, Kind::Buffer)
                    .unwrap()
                    .buffer_target,
                gl::ARRAY_BUFFER
            );
            assert_eq!(
                command(
                    &mut context,
                    "bindBuffer",
                    &[gl::ELEMENT_ARRAY_BUFFER as i64, other],
                    ""
                ),
                Err(gl::INVALID_OPERATION)
            );
            assert_eq!(
                call(
                    &mut context,
                    "getParameter",
                    &[gl::ELEMENT_ARRAY_BUFFER_BINDING as i64],
                    ""
                ),
                Value::Null
            );
            let indices = call(&mut context, "createBuffer", &[], "")
                .as_i64()
                .unwrap();
            call(
                &mut context,
                "bindBuffer",
                &[gl::ELEMENT_ARRAY_BUFFER as i64, indices],
                "",
            );
            call(&mut context, "bindBuffer", &[copy as i64, indices], "");
            assert_eq!(
                context
                    .objects
                    .get(indices as u32, Kind::Buffer)
                    .unwrap()
                    .buffer_target,
                gl::ELEMENT_ARRAY_BUFFER
            );
            assert_eq!(
                command(
                    &mut context,
                    "bindBuffer",
                    &[gl::ARRAY_BUFFER as i64, indices],
                    ""
                ),
                Err(gl::INVALID_OPERATION)
            );
            call(
                &mut context,
                "bindBuffer",
                &[gl::ELEMENT_ARRAY_BUFFER as i64, 0],
                "",
            );
        }
    });
}

#[test]
fn webgl2_indexed_ranges_can_precede_storage_and_keep_explicit_extent_after_allocation() {
    session::run_native_test(|| {
        let mut context = version_two();
        for (target, binding, start, size) in [
            (0x8a11, 0x8a28, 0x8a29, 0x8a2a),
            (0x8c8e, 0x8c8f, 0x8c84, 0x8c85),
        ] {
            let id = call(&mut context, "createBuffer", &[], "")
                .as_i64()
                .unwrap();
            command(&mut context, "bindBufferRange", &[target, 0, id, 0, 64], "").unwrap();
            assert_eq!(
                call(&mut context, "getIndexedParameter", &[binding, 0], ""),
                json!(id)
            );
            assert_eq!(
                call(&mut context, "getIndexedParameter", &[start, 0], ""),
                json!(0)
            );
            assert_eq!(
                call(&mut context, "getIndexedParameter", &[size, 0], ""),
                json!(64)
            );
            assert_eq!(
                call(
                    &mut context,
                    "getBufferParameter",
                    &[target, gl::BUFFER_SIZE as i64],
                    ""
                ),
                json!(0)
            );
            call(
                &mut context,
                "bufferData",
                &[target, 16, gl::DYNAMIC_DRAW as i64],
                "",
            );
            assert_eq!(
                call(&mut context, "getIndexedParameter", &[size, 0], ""),
                json!(64)
            );
            assert_eq!(
                call(
                    &mut context,
                    "getBufferParameter",
                    &[target, gl::BUFFER_SIZE as i64],
                    ""
                ),
                json!(16)
            );
            assert_eq!(
                command(
                    &mut context,
                    "bindBufferRange",
                    &[target, 0, id, 0, i64::MAX],
                    ""
                ),
                if target == 0x8c8e {
                    Err(gl::INVALID_VALUE)
                } else {
                    Ok(Value::Null)
                }
            );
            call(&mut context, "bindBufferBase", &[target, 0, 0], "");
        }
    });
}

#[test]
fn webgl2_indexed_range_uniform_draw_rejects_insufficient_real_storage() {
    session::run_native_test(|| {
        let mut context = version_two();
        super::core_uniform_tests::program(
            &mut context,
            super::core_uniform_tests::VERTEX,
            "#version 300 es\nprecision highp float;layout(std140) uniform Paint {vec4 color;};out vec4 result;void main(){result=color;}",
        );
        let id = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        command(&mut context, "bindBufferRange", &[0x8a11, 0, id, 0, 64], "").unwrap();
        let draw = [gl::TRIANGLES as i64, 0, 3];
        assert_eq!(
            command(&mut context, "drawArrays", &draw, ""),
            Err(gl::INVALID_OPERATION)
        );
        call(
            &mut context,
            "bufferData",
            &[0x8a11, 8, gl::DYNAMIC_DRAW as i64],
            "",
        );
        assert_eq!(
            command(&mut context, "drawArrays", &draw, ""),
            Err(gl::INVALID_OPERATION)
        );
        call(
            &mut context,
            "bufferData",
            &[0x8a11, 64, gl::DYNAMIC_DRAW as i64],
            "",
        );
        command(&mut context, "drawArrays", &draw, "").unwrap();
        call(
            &mut context,
            "bufferData",
            &[0x8a11, 8, gl::DYNAMIC_DRAW as i64],
            "",
        );
        assert_eq!(
            command(&mut context, "drawArrays", &draw, ""),
            Err(gl::INVALID_OPERATION)
        );
    });
}

#[test]
fn webgl2_indexed_range_capture_draw_rejects_ranges_outside_real_storage() {
    session::run_native_test(|| {
        use super::transform_feedback_tests::{INTERLEAVED, TARGET, shader};
        let mut context = version_two();
        shader(&mut context, INTERLEAVED, r#"["captured"]"#);
        let id = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        command(&mut context, "bindBufferRange", &[TARGET, 0, id, 0, 64], "").unwrap();
        command(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        )
        .unwrap();
        assert_eq!(
            command(&mut context, "drawArrays", &[gl::POINTS as i64, 0, 1], ""),
            Err(gl::INVALID_OPERATION)
        );
        command(&mut context, "endTransformFeedback", &[], "").unwrap();
        call(
            &mut context,
            "bufferData",
            &[TARGET, 16, gl::DYNAMIC_DRAW as i64],
            "",
        );
        command(
            &mut context,
            "bindBufferRange",
            &[TARGET, 0, id, 16, 64],
            "",
        )
        .unwrap();
        command(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        )
        .unwrap();
        assert_eq!(
            command(&mut context, "drawArrays", &[gl::POINTS as i64, 0, 1], ""),
            Err(gl::INVALID_OPERATION)
        );
        command(&mut context, "endTransformFeedback", &[], "").unwrap();
        command(&mut context, "bindBufferRange", &[TARGET, 0, id, 0, 64], "").unwrap();
        command(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        )
        .unwrap();
        command(&mut context, "drawArrays", &[gl::POINTS as i64, 0, 1], "").unwrap();
        command(&mut context, "endTransformFeedback", &[], "").unwrap();
        let bytes = super::transform_feedback_tests::read(&mut context, id, 16);
        super::transform_feedback_tests::assert_float_capture(&bytes[..8], &[0.5, 0.0]);
        assert!(bytes[8..].iter().all(|value| *value == 0));
    });
}
