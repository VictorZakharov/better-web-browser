//! Preservation is verified with real GPU pixels, including exact integer values.
use super::api_version_tests::{call, version_two};
use super::framebuffer_guard::{DRAW, READ};
use super::typed_framebuffer_tests::{clear, image, read};
use super::*;

pub(super) fn hint(context: &mut WebGl, target: u32, sub: bool, attachment: u32) {
    let args = if sub {
        vec![target as i64, -1, -1, 6, 6, attachment as i64]
    } else {
        vec![target as i64, attachment as i64]
    };
    call(
        context,
        if sub {
            "invalidateSubFramebuffer"
        } else {
            "invalidateFramebuffer"
        },
        &args,
        "",
    );
}

#[test]
fn webgl2_invalidation_preserves_integer_color_bits_and_bindings_for_every_target() {
    session::run_native_test(|| {
        let mut context = version_two();
        for (format, op, values, kind) in [
            (
                0x8d82,
                "clearBufferiv",
                [-2147483648, -17, 0, 2147483647],
                gl::INT,
            ),
            (
                0x8d70,
                "clearBufferuiv",
                [0, 0x80000000, 0xfffffffe, 0xffffffff],
                gl::UNSIGNED_INT,
            ),
        ] {
            image(&mut context, format);
            let mut args = vec![0x1800, 0];
            args.extend(values);
            clear(&mut context, op, &args, &[]).unwrap();
            let expected = read(&mut context, 0x8d99, kind, 256).unwrap();
            assert!(!expected.iter().all(|&byte| byte == 0));
            let draw = context.framebuffer;
            let bytes = context.resource_bytes;
            for target in [gl::FRAMEBUFFER, DRAW, READ] {
                for sub in [false, true] {
                    hint(&mut context, target, sub, gl::COLOR_ATTACHMENT0);
                    assert_eq!(read(&mut context, 0x8d99, kind, 256).unwrap(), expected);
                    assert_eq!(context.framebuffer, draw);
                    assert_eq!(context.read_framebuffer, draw);
                    assert_eq!(context.resource_bytes, bytes);
                }
            }
        }
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_default_invalidation_preserves_color_even_with_scissor_and_masks() {
    session::run_native_test(|| {
        let mut context = version_two();
        clear(&mut context, "clearColor", &[], &[1., 0., 1., 1.]).unwrap();
        call(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
        let expected = read(&mut context, gl::RGBA, gl::UNSIGNED_BYTE, 64).unwrap();
        assert_eq!(&expected[..4], &[255, 0, 255, 255]);
        call(&mut context, "enable", &[gl::SCISSOR_TEST as i64], "");
        call(&mut context, "scissor", &[1, 1, 1, 1], "");
        call(&mut context, "colorMask", &[0, 0, 0, 0], "");
        for target in [gl::FRAMEBUFFER, DRAW, READ] {
            for sub in [false, true] {
                for attachment in [0x1800, 0x1801, 0x1802] {
                    hint(&mut context, target, sub, attachment);
                    assert_eq!(
                        read(&mut context, gl::RGBA, gl::UNSIGNED_BYTE, 64).unwrap(),
                        expected
                    );
                }
            }
        }
        assert_eq!(
            call(&mut context, "getParameter", &[gl::SCISSOR_BOX as i64], ""),
            json!([1, 1, 1, 1])
        );
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::COLOR_WRITEMASK as i64],
                ""
            ),
            json!([false, false, false, false])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
