//! Built-in capture outputs follow the linker, not identifier-declaration rules.
use super::api_version_tests::{call, version_two};
use super::transform_feedback_tests::*;
use super::*;

#[test]
fn webgl2_captures_actual_builtin_position_and_point_size() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = shader(
            &mut context,
            INTERLEAVED,
            r#"["gl_Position","gl_PointSize"]"#,
        );
        for (index, name, kind) in [
            (0, "gl_Position", gl::FLOAT_VEC4),
            (1, "gl_PointSize", gl::FLOAT),
        ] {
            assert_eq!(
                call(
                    &mut context,
                    "getTransformFeedbackVarying",
                    &[program as i64, index],
                    ""
                ),
                json!({"name":name,"size":1,"type":kind})
            );
        }
        let tf = object(&mut context);
        call(&mut context, "bindTransformFeedback", &[OBJECT, tf], "");
        let output = buffer(&mut context, 40);
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        call(&mut context, "enable", &[0x8c89], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        call(&mut context, "drawArrays", &[gl::POINTS as i64, 0, 2], "");
        call(&mut context, "endTransformFeedback", &[], "");
        assert_float_capture(
            &read(&mut context, output, 40),
            &[0., 0., 0., 1., 1., 0., 0., 0., 1., 1.],
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_builtin_capture_can_begin_before_storage_without_mutating_active_bindings() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context, INTERLEAVED, r#"["gl_Position"]"#);
        let tf = object(&mut context);
        call(&mut context, "bindTransformFeedback", &[OBJECT, tf], "");
        let output = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        let other = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        for id in [output, other, 0] {
            assert_eq!(
                command(&mut context, "bindBufferBase", &[TARGET, 0, id], ""),
                Err(gl::INVALID_OPERATION)
            );
            assert_eq!(
                command(&mut context, "bindBufferRange", &[TARGET, 0, id, 0, 64], ""),
                Err(gl::INVALID_OPERATION)
            );
            assert_eq!(
                call(&mut context, "getIndexedParameter", &[0x8c8f, 0], ""),
                json!(output)
            );
        }
        call(&mut context, "pauseTransformFeedback", &[], "");
        assert_eq!(
            command(&mut context, "bindBufferBase", &[TARGET, 0, other], ""),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "endTransformFeedback", &[], "");
        call(&mut context, "bindBufferBase", &[TARGET, 0, other], "");
    });
}
