//! Hints must not reset HDR, depth/stencil, multisample, or independently bound images.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::framebuffer_guard::{DRAW, READ};
use super::framebuffer_invalidation_pixel_tests::hint;
use super::multisample_tests::{attach, color, framebuffer, pixels, renderbuffer};
use super::typed_framebuffer_tests::{clear, image, read};
use super::*;

#[test]
fn webgl2_invalidation_preserves_hdr_without_clamping_or_reallocation() {
    session::run_native_test(|| {
        let mut context = version_two();
        assert_eq!(
            call(
                &mut context,
                "enableExtension",
                &[],
                "EXT_color_buffer_float"
            ),
            json!(true)
        );
        for format in [0x881a, 0x8814] {
            image(&mut context, format);
            clear(
                &mut context,
                "clearBufferfv",
                &[0x1800, 0],
                &[2.5, -1., 0.25, 1.],
            )
            .unwrap();
            let expected = read(&mut context, gl::RGBA, gl::FLOAT, 256).unwrap();
            let value = [2.5f32, -1., 0.25, 1.].map(f32::to_ne_bytes).concat();
            assert!(expected.chunks_exact(16).all(|pixel| pixel == value));
            let charged = context.resource_bytes;
            for sub in [false, true] {
                hint(&mut context, gl::FRAMEBUFFER, sub, gl::COLOR_ATTACHMENT0);
                assert_eq!(
                    read(&mut context, gl::RGBA, gl::FLOAT, 256).unwrap(),
                    expected
                );
                assert_eq!(context.resource_bytes, charged);
            }
        }
    });
}

#[test]
fn webgl2_invalidation_preserves_default_depth_and_stencil_test_results() {
    session::run_native_test(|| {
        let mut context = WebGl::new(
            4,
            4,
            Options {
                api: ApiVersion::Two,
                stencil: true,
                ..Options::default()
            },
        )
        .unwrap();
        color(&mut context, [1., 0., 0., 1.]);
        clear(&mut context, "clearDepth", &[], &[0.25]).unwrap();
        call(&mut context, "clearStencil", &[5], "");
        call(
            &mut context,
            "clear",
            &[(gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT) as i64],
            "",
        );
        program(
            &mut context,
            VERTEX,
            "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(0,1,0,1);}",
        );
        for sub in [false, true] {
            hint(&mut context, gl::FRAMEBUFFER, sub, 0x1801);
            hint(&mut context, gl::FRAMEBUFFER, sub, 0x1802);
        }
        call(&mut context, "enable", &[gl::DEPTH_TEST as i64], "");
        call(&mut context, "depthFunc", &[gl::LESS as i64], "");
        call(
            &mut context,
            "drawArrays",
            &[gl::TRIANGLES as i64, 0, 3],
            "",
        );
        assert!(
            pixels(&mut context)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255]),
            "depth 0.5 must fail against preserved 0.25"
        );
        call(&mut context, "disable", &[gl::DEPTH_TEST as i64], "");
        call(&mut context, "enable", &[gl::STENCIL_TEST as i64], "");
        call(&mut context, "stencilFunc", &[gl::EQUAL as i64, 5, 255], "");
        call(
            &mut context,
            "drawArrays",
            &[gl::TRIANGLES as i64, 0, 3],
            "",
        );
        assert!(
            pixels(&mut context)
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255]),
            "stencil 5 must pass against preserved 5"
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_invalidation_preserves_multisample_storage_through_native_resolve() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source = framebuffer(&mut context, gl::FRAMEBUFFER);
        let storage = renderbuffer(&mut context, 4);
        attach(&mut context, gl::FRAMEBUFFER, storage);
        color(&mut context, [0., 1., 1., 1.]);
        let charged = context.resource_bytes;
        hint(&mut context, gl::FRAMEBUFFER, false, gl::COLOR_ATTACHMENT0);
        hint(&mut context, gl::FRAMEBUFFER, true, gl::COLOR_ATTACHMENT0);
        assert_eq!(context.resource_bytes, charged);
        image(&mut context, 0x8058);
        let destination = context.framebuffer;
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, source as i64],
            "",
        );
        call(
            &mut context,
            "blitFramebuffer",
            &[
                0,
                0,
                4,
                4,
                0,
                0,
                4,
                4,
                gl::COLOR_BUFFER_BIT as i64,
                gl::NEAREST as i64,
            ],
            "",
        );
        assert_eq!(context.framebuffer, destination);
        assert_eq!(context.read_framebuffer, source);
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, destination as i64],
            "",
        );
        assert!(
            pixels(&mut context)
                .chunks_exact(4)
                .all(|p| p == [0, 255, 255, 255])
        );
    });
}

#[test]
fn webgl2_invalidation_does_not_confuse_separate_read_and_draw_attachments() {
    session::run_native_test(|| {
        let mut context = version_two();
        image(&mut context, 0x8058);
        let first = context.framebuffer;
        color(&mut context, [1., 0., 0., 1.]);
        image(&mut context, 0x8058);
        let second = context.framebuffer;
        color(&mut context, [0., 0., 1., 1.]);
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, first as i64],
            "",
        );
        let charged = context.resource_bytes;
        for target in [READ, DRAW, gl::FRAMEBUFFER] {
            for sub in [false, true] {
                hint(&mut context, target, sub, gl::COLOR_ATTACHMENT0);
                assert_eq!(context.read_framebuffer, first);
                assert_eq!(context.framebuffer, second);
                assert!(
                    pixels(&mut context)
                        .chunks_exact(4)
                        .all(|p| p == [255, 0, 0, 255])
                );
            }
        }
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, second as i64],
            "",
        );
        assert!(
            pixels(&mut context)
                .chunks_exact(4)
                .all(|p| p == [0, 0, 255, 255])
        );
        assert_eq!(context.resource_bytes, charged);
    });
}
