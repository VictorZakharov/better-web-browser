//! Feedback ownership, invalid-operation atomicity and retained buffer storage.
use super::api_version_tests::{call, version_two};
use super::transform_feedback_tests::*;
use super::*;

#[test]
fn webgl2_transform_objects_keep_separate_indexed_bindings_and_generic_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        let first = object(&mut context);
        let second = object(&mut context);
        assert_eq!(
            call(&mut context, "isTransformFeedback", &[first], ""),
            json!(false)
        );
        call(&mut context, "bindTransformFeedback", &[OBJECT, first], "");
        let a = buffer(&mut context, 32);
        call(&mut context, "bindBufferRange", &[TARGET, 0, a, 4, 16], "");
        call(&mut context, "bindTransformFeedback", &[OBJECT, second], "");
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c8f, 0], ""),
            Value::Null
        );
        let b = buffer(&mut context, 64);
        call(&mut context, "bindBufferBase", &[TARGET, 0, b], "");
        call(&mut context, "bindTransformFeedback", &[OBJECT, first], "");
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c8f, 0], ""),
            json!(a)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c84, 0], ""),
            json!(4)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c85, 0], ""),
            json!(16)
        );
        assert_eq!(call(&mut context, "getParameter", &[0x8c8f], ""), json!(b));
        call(&mut context, "bindTransformFeedback", &[OBJECT, 0], "");
        assert_eq!(
            call(&mut context, "getParameter", &[0x8e25], ""),
            Value::Null
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c8f, 0], ""),
            Value::Null
        );
        assert_eq!(
            call(&mut context, "isTransformFeedback", &[first], ""),
            json!(true)
        );
    });
}

#[test]
fn webgl2_transform_lifecycle_errors_leave_active_capture_and_bindings_unchanged() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = shader(&mut context, INTERLEAVED, r#"["captured"]"#);
        let feedback = object(&mut context);
        call(
            &mut context,
            "bindTransformFeedback",
            &[OBJECT, feedback],
            "",
        );
        let id = buffer(&mut context, 16);
        call(&mut context, "bindBufferBase", &[TARGET, 0, id], "");
        assert_eq!(
            command(&mut context, "endTransformFeedback", &[], ""),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(
                &mut context,
                "beginTransformFeedback",
                &[gl::LINE_STRIP as i64],
                ""
            ),
            Err(gl::INVALID_ENUM)
        );
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        assert_eq!(
            command(
                &mut context,
                "beginTransformFeedback",
                &[gl::POINTS as i64],
                ""
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut context, "bindTransformFeedback", &[OBJECT, 0], ""),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut context, "deleteTransformFeedback", &[feedback], ""),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut context, "linkProgram", &[program as i64], ""),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut context, "bindBufferBase", &[TARGET, 0, 0], ""),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut context, "getBufferSubData", &[TARGET, 0, 16], ""),
            Err(gl::INVALID_OPERATION)
        );
        draw(&mut context, 0, 1);
        call(&mut context, "pauseTransformFeedback", &[], "");
        assert_eq!(
            command(&mut context, "pauseTransformFeedback", &[], ""),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut context, "bindBufferBase", &[TARGET, 0, 0], ""),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut context, "getBufferSubData", &[TARGET, 0, 16], ""),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "resumeTransformFeedback", &[], "");
        draw(&mut context, 1, 1);
        call(&mut context, "endTransformFeedback", &[], "");
        assert_eq!(
            read(&mut context, id, 16),
            [0.5f32, 0., 1.5, -1.].map(f32::to_ne_bytes).concat()
        );
        call(&mut context, "deleteTransformFeedback", &[feedback], "");
        assert_eq!(
            call(&mut context, "getParameter", &[0x8e25], ""),
            Value::Null
        );
    });
}

#[test]
fn webgl2_transform_range_errors_and_duplicate_capture_bindings_are_atomic() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context, SEPARATE, r#"["captured","identity"]"#);
        let id = buffer(&mut context, 32);
        for (offset, size) in [(-1, 4), (2, 4), (4, 6), (0, 0), (32, 4), (i64::MAX, 4)] {
            assert_eq!(
                command(
                    &mut context,
                    "bindBufferRange",
                    &[TARGET, 0, id, offset, size],
                    ""
                ),
                Err(gl::INVALID_VALUE)
            );
        }
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c8f, 0], ""),
            Value::Null
        );
        assert_eq!(
            command(&mut context, "getIndexedParameter", &[0x8c8f, 999], ""),
            Err(gl::INVALID_VALUE)
        );
        call(&mut context, "bindBufferBase", &[TARGET, 0, id], "");
        call(
            &mut context,
            "bindBufferRange",
            &[TARGET, 1, id, 16, 16],
            "",
        );
        assert_eq!(
            command(
                &mut context,
                "beginTransformFeedback",
                &[gl::POINTS as i64],
                ""
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8e24], ""),
            json!(false)
        );
        let separate = buffer(&mut context, 16);
        call(&mut context, "bindBufferBase", &[TARGET, 1, separate], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        draw(&mut context, 0, 1);
        call(&mut context, "endTransformFeedback", &[], "");
        assert_eq!(
            &read(&mut context, separate, 16)[..4],
            &0x8000_0000u32.to_ne_bytes()
        );
    });
}

#[test]
fn webgl2_transform_unbound_objects_retain_deleted_buffer_storage_until_retirement() {
    session::run_native_test(|| {
        let mut context = version_two();
        let feedback = object(&mut context);
        call(
            &mut context,
            "bindTransformFeedback",
            &[OBJECT, feedback],
            "",
        );
        let id = buffer(&mut context, 16);
        call(&mut context, "bindBufferBase", &[TARGET, 0, id], "");
        call(&mut context, "bindTransformFeedback", &[OBJECT, 0], "");
        call(&mut context, "deleteBuffer", &[id], "");
        assert!(
            context
                .objects
                .get(id as u32, Kind::Buffer)
                .unwrap()
                .pending_delete
        );
        call(
            &mut context,
            "bindTransformFeedback",
            &[OBJECT, feedback],
            "",
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c8f, 0], ""),
            json!(id)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c85, 0], ""),
            json!(16)
        );
        assert_eq!(
            command(&mut context, "bindBufferBase", &[TARGET, 1, id], ""),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "deleteTransformFeedback", &[feedback], "");
        assert!(context.objects.get(id as u32, Kind::Buffer).is_err());
        assert_eq!(
            call(&mut context, "isTransformFeedback", &[feedback], ""),
            json!(false)
        );
    });
}

#[test]
fn webgl2_transform_aliases_are_rejected_even_when_capture_is_not_active() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context, INTERLEAVED, r#"["captured"]"#);
        let id = buffer(&mut context, 16);
        call(&mut context, "bindBufferBase", &[TARGET, 0, id], "");
        call(
            &mut context,
            "bindBuffer",
            &[super::core_buffers::COPY_READ as i64, id],
            "",
        );
        assert_eq!(
            command(
                &mut context,
                "getBufferSubData",
                &[super::core_buffers::COPY_READ as i64, 0, 16],
                ""
            ),
            Err(gl::INVALID_OPERATION)
        );
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        assert_eq!(
            command(&mut context, "drawArrays", &[gl::POINTS as i64, 0, 1], ""),
            Err(gl::INVALID_OPERATION)
        );
        call(
            &mut context,
            "bindBuffer",
            &[super::core_buffers::COPY_READ as i64, 0],
            "",
        );
        draw(&mut context, 0, 1);
        call(&mut context, "endTransformFeedback", &[], "");
        assert_eq!(
            &read(&mut context, id, 16)[..8],
            &[0.5f32.to_ne_bytes(), 0f32.to_ne_bytes()].concat()
        );
    });
}

#[test]
fn webgl2_transform_objects_and_varying_lists_reject_foreign_or_malformed_inputs() {
    session::run_native_test(|| {
        let mut peer = version_two();
        let foreign = object(&mut peer);
        let mut context = version_two();
        assert_eq!(
            command(
                &mut context,
                "bindTransformFeedback",
                &[OBJECT, foreign],
                ""
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            call(&mut context, "isTransformFeedback", &[foreign], ""),
            json!(false)
        );
        let program = shader(&mut context, INTERLEAVED, r#"["captured"]"#);
        for names in ["null", r#"[1]"#, r#"["a\u0000b"]"#, r#"["gl_Position"]"#] {
            assert_eq!(
                command(
                    &mut context,
                    "transformFeedbackVaryings",
                    &[program as i64, INTERLEAVED],
                    names
                ),
                Err(gl::INVALID_VALUE)
            );
        }
        let long = json!(["x".repeat(1025)]).to_string();
        assert_eq!(
            command(
                &mut context,
                "transformFeedbackVaryings",
                &[program as i64, INTERLEAVED],
                &long
            ),
            Err(gl::INVALID_VALUE)
        );
        let many = json!(["x", "y", "z", "w", "v"]).to_string();
        assert_eq!(
            command(
                &mut context,
                "transformFeedbackVaryings",
                &[program as i64, SEPARATE],
                &many
            ),
            Err(gl::INVALID_VALUE)
        );
        assert_eq!(
            command(
                &mut context,
                "transformFeedbackVaryings",
                &[program as i64, 0],
                "[]"
            ),
            Err(gl::INVALID_ENUM)
        );
        let mut one = WebGl::new(1, 1, Options::default()).unwrap();
        assert_eq!(
            command(&mut one, "createTransformFeedback", &[], ""),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            command(&mut one, "getParameter", &[0x8e25], ""),
            Err(gl::INVALID_ENUM)
        );
    });
}
