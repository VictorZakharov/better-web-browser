use super::super::api_version_tests::{call, version_two};
use super::*;

fn dispatch(context: &mut WebGl, op: &str, input: &[i64]) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: input.into(),
            f: vec![],
            text: String::new(),
        },
        None,
    )
}

fn admit(context: &mut WebGl) {
    assert!(
        context.extensions.available_indexed_blend,
        "pinned ANGLE must provide the real indexed entry points and advertised extension"
    );
    assert_eq!(
        call(context, "enableExtension", &[], "OES_draw_buffers_indexed"),
        json!(true)
    );
}

#[test]
fn indexed_blending_requires_webgl2_native_support_and_explicit_admission() {
    super::super::session::run_native_test(|| {
        let mut one = WebGl::new(4, 4, super::super::Options::default()).unwrap();
        assert_eq!(
            call(&mut one, "enableExtension", &[], "OES_draw_buffers_indexed"),
            json!(false)
        );
        assert!(
            !call(&mut one, "supportedExtensions", &[], "")
                .as_array()
                .unwrap()
                .contains(&json!("OES_draw_buffers_indexed"))
        );
        let mut context = version_two();
        assert_eq!(
            dispatch(&mut context, "blendFunciOES", &[0, 1, 0]),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            dispatch(&mut context, "getIndexedParameter", &[0x80c9, 0]),
            Err(gl::INVALID_ENUM)
        );
        admit(&mut context);
        assert_eq!(
            dispatch(&mut context, "getIndexedParameter", &[0x80c9, 0]),
            Ok(json!(gl::ONE))
        );
        assert_eq!(
            dispatch(&mut context, "getIndexedParameter", &[gl::BLEND as i64, 0]),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(gl::NO_ERROR));
    });
}

#[test]
fn indexed_factors_equations_and_masks_are_independent_then_global_calls_broadcast() {
    super::super::session::run_native_test(|| {
        let mut context = version_two();
        admit(&mut context);
        call(
            &mut context,
            "blendFuncSeparateiOES",
            &[
                1,
                gl::SRC_ALPHA as i64,
                gl::ONE_MINUS_SRC_ALPHA as i64,
                1,
                0,
            ],
            "",
        );
        call(
            &mut context,
            "blendEquationSeparateiOES",
            &[
                1,
                gl::FUNC_SUBTRACT as i64,
                gl::FUNC_REVERSE_SUBTRACT as i64,
            ],
            "",
        );
        call(&mut context, "colorMaskiOES", &[1, 1, 0, 1, 0], "");
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x80c9, 0], ""),
            json!(gl::ONE)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x80c9, 1], ""),
            json!(gl::SRC_ALPHA)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8009, 1], ""),
            json!(gl::FUNC_SUBTRACT)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x883d, 1], ""),
            json!(gl::FUNC_REVERSE_SUBTRACT)
        );
        assert_eq!(
            call(
                &mut context,
                "getIndexedParameter",
                &[gl::COLOR_WRITEMASK as i64, 0],
                ""
            ),
            json!([true, true, true, true])
        );
        assert_eq!(
            call(
                &mut context,
                "getIndexedParameter",
                &[gl::COLOR_WRITEMASK as i64, 1],
                ""
            ),
            json!([true, false, true, false])
        );
        call(&mut context, "blendFunc", &[0, 1], "");
        call(&mut context, "blendEquation", &[gl::FUNC_ADD as i64], "");
        call(&mut context, "colorMask", &[0, 1, 0, 1], "");
        for index in 0..context.extensions.max_draw_buffers {
            assert_eq!(
                call(
                    &mut context,
                    "getIndexedParameter",
                    &[0x80c9, index as i64],
                    ""
                ),
                json!(0)
            );
            assert_eq!(
                call(
                    &mut context,
                    "getIndexedParameter",
                    &[0x8009, index as i64],
                    ""
                ),
                json!(gl::FUNC_ADD)
            );
            assert_eq!(
                call(
                    &mut context,
                    "getIndexedParameter",
                    &[gl::COLOR_WRITEMASK as i64, index as i64],
                    ""
                ),
                json!([false, true, false, true])
            );
        }
    });
}

#[test]
fn invalid_indexed_updates_preserve_state_and_private_retirement_preserves_every_mask() {
    super::super::session::run_native_test(|| {
        let mut context = version_two();
        admit(&mut context);
        let limit = context.extensions.max_draw_buffers as i64;
        assert_eq!(
            dispatch(&mut context, "blendFunciOES", &[limit, 1, 0]),
            Err(gl::INVALID_VALUE)
        );
        assert_eq!(
            dispatch(&mut context, "enableiOES", &[gl::SCISSOR_TEST as i64, 0]),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            dispatch(&mut context, "blendEquationiOES", &[0, 999]),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            call(&mut context, "getError", &[], ""),
            json!(gl::INVALID_ENUM)
        );
        assert_eq!(
            dispatch(
                &mut context,
                "blendFunciOES",
                &[0, gl::CONSTANT_COLOR as i64, gl::CONSTANT_ALPHA as i64]
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            call(&mut context, "getError", &[], ""),
            json!(gl::INVALID_OPERATION)
        );
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x80c9, 0], ""),
            json!(gl::ONE)
        );
        for index in 0..limit {
            call(
                &mut context,
                "colorMaskiOES",
                &[index, index % 2, 0, 1, 0],
                "",
            );
        }
        context.clear_default_surface();
        for index in 0..limit {
            assert_eq!(
                call(
                    &mut context,
                    "getIndexedParameter",
                    &[gl::COLOR_WRITEMASK as i64, index],
                    ""
                ),
                json!([index % 2 != 0, false, true, false])
            );
        }
        assert_eq!(call(&mut context, "getError", &[], ""), json!(gl::NO_ERROR));
    });
}
