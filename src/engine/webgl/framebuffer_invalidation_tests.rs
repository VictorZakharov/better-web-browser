//! Logical default/user targets, attachment domains and atomic error validation.
use super::api_version_tests::{call, version_two};
use super::framebuffer_guard::{DRAW, READ};
use super::*;

fn invalidate(context: &mut WebGl, sub: bool, integers: &[i64]) -> Result<Value> {
    context.dispatch(
        &Command {
            op: if sub {
                "invalidateSubFramebuffer"
            } else {
                "invalidateFramebuffer"
            }
            .into(),
            i: integers.into(),
            f: vec![],
            text: String::new(),
        },
        None,
    )
}

#[test]
fn webgl2_invalidation_default_attachments_do_not_expose_private_color_attachment() {
    session::run_native_test(|| {
        let mut context = version_two();
        for target in [gl::FRAMEBUFFER, READ, DRAW] {
            for attachments in [
                vec![],
                vec![0x1800],
                vec![0x1801, 0x1802],
                vec![0x1800, 0x1800, 0x1802],
            ] {
                let mut args = vec![target as i64];
                args.extend_from_slice(&attachments);
                assert_eq!(invalidate(&mut context, false, &args), Ok(Value::Null));
            }
            for invalid in [
                gl::COLOR_ATTACHMENT0,
                gl::DEPTH_ATTACHMENT,
                gl::STENCIL_ATTACHMENT,
                0x821a,
                gl::BACK,
                gl::NONE,
            ] {
                assert_eq!(
                    invalidate(&mut context, false, &[target as i64, invalid as i64]),
                    Err(gl::INVALID_ENUM)
                );
            }
        }
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_invalidation_user_attachment_limit_has_the_exact_error_class() {
    session::run_native_test(|| {
        let mut context = version_two();
        let framebuffer = call(&mut context, "createFramebuffer", &[], "")
            .as_i64()
            .unwrap();
        call(
            &mut context,
            "bindFramebuffer",
            &[DRAW as i64, framebuffer],
            "",
        );
        // Hints do not require framebuffer completeness or attached images.
        for attachment in [gl::DEPTH_ATTACHMENT, gl::STENCIL_ATTACHMENT, 0x821a] {
            assert_eq!(
                invalidate(&mut context, false, &[DRAW as i64, attachment as i64]),
                Ok(Value::Null)
            );
        }
        for index in 0..32 {
            let point = gl::COLOR_ATTACHMENT0 + index;
            assert_eq!(
                invalidate(&mut context, false, &[DRAW as i64, point as i64]),
                if context.color_attachment_allowed(point) {
                    Ok(Value::Null)
                } else {
                    Err(gl::INVALID_OPERATION)
                }
            );
        }
        for invalid in [
            0x1800,
            0x1801,
            0x1802,
            gl::BACK,
            // COLOR_ATTACHMENT0 + 32 is DEPTH_ATTACHMENT, not an invalid enum.
            gl::COLOR_ATTACHMENT0 + 33,
            u32::MAX,
        ] {
            assert_eq!(
                invalidate(&mut context, false, &[DRAW as i64, invalid as i64]),
                Err(gl::INVALID_ENUM)
            );
        }
        assert_eq!(
            invalidate(
                &mut context,
                false,
                &[READ as i64, gl::COLOR_ATTACHMENT0 as i64]
            ),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            invalidate(&mut context, false, &[READ as i64, 0x1800]),
            Ok(Value::Null)
        );
    });
}

#[test]
fn webgl2_sub_invalidation_validates_signed_dimensions_but_does_not_overflow_origins() {
    session::run_native_test(|| {
        let mut context = version_two();
        for target in [gl::FRAMEBUFFER, READ, DRAW] {
            for (x, y, width, height) in [
                (-2, -3, 5, 7),
                (i32::MAX, i32::MAX, i32::MAX, i32::MAX),
                (i32::MIN, i32::MIN, 0, 0),
            ] {
                assert_eq!(
                    invalidate(
                        &mut context,
                        true,
                        &[
                            target as i64,
                            x as i64,
                            y as i64,
                            width as i64,
                            height as i64,
                            0x1800
                        ]
                    ),
                    Ok(Value::Null)
                );
            }
            for dimensions in [
                [0, 0, -1, 1],
                [0, 0, 1, -1],
                [0, 0, -1, -1],
                [0, 0, i64::MAX, 1],
                [i64::MAX, 0, 1, 1],
            ] {
                let mut args = vec![target as i64];
                args.extend(dimensions);
                assert_eq!(
                    invalidate(&mut context, true, &args),
                    Err(gl::INVALID_VALUE)
                );
            }
            for short in 1..5 {
                let args = [target as i64, 0, 0, 1, 1];
                assert_eq!(
                    invalidate(&mut context, true, &args[..short]),
                    Err(gl::INVALID_VALUE)
                );
            }
        }
        assert_eq!(
            invalidate(&mut context, false, &[0, 0x1800]),
            Err(gl::INVALID_ENUM)
        );
        let mut one = WebGl::new(1, 1, Options::default()).unwrap();
        for sub in [false, true] {
            assert_eq!(
                invalidate(&mut one, sub, &[gl::FRAMEBUFFER as i64, 0x1800]),
                Err(gl::INVALID_OPERATION)
            );
        }
    });
}
