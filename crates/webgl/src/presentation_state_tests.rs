//! Drawing-buffer retirement is not an author clear and ignores discard/masks.
use super::api_version_tests::call;
use super::framebuffer_guard::{DRAW, READ};
use super::*;

const DISCARD: u32 = 0x8c89;

#[test]
fn native_presentation_generation_tracks_default_writes_not_reads_or_fbo_work() {
    session::run_native_test(|| {
        for preserve in [false, true] {
            let mut context = context(false, preserve, true);
            assert_eq!(
                call(&mut context, "drawingBufferDirty", &[], ""),
                json!(true)
            );
            context.presented();
            assert_eq!(
                call(&mut context, "drawingBufferDirty", &[], ""),
                json!(false)
            );
            context.surface.snapshot().unwrap();
            call(&mut context, "getError", &[], "");
            assert_eq!(
                call(&mut context, "drawingBufferDirty", &[], ""),
                json!(false)
            );
            let fbo = call(&mut context, "createFramebuffer", &[], "")
                .as_u64()
                .unwrap();
            call(
                &mut context,
                "bindFramebuffer",
                &[DRAW as i64, fbo as i64],
                "",
            );
            // Invalid FBO draws remain ordinary GL errors; they are not Canvas
            // writes and must not publish the implicit default-buffer clear.
            assert_eq!(
                context.dispatch(
                    &Command {
                        op: "clear".into(),
                        i: vec![gl::COLOR_BUFFER_BIT as i64],
                        f: vec![],
                        text: String::new(),
                    },
                    None
                ),
                Err(gl::INVALID_FRAMEBUFFER_OPERATION)
            );
            assert_eq!(
                call(&mut context, "drawingBufferDirty", &[], ""),
                json!(false)
            );
            call(&mut context, "bindFramebuffer", &[DRAW as i64, 0], "");
            call(
                &mut context,
                "bindFramebuffer",
                &[READ as i64, fbo as i64],
                "",
            );
            call(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
            assert_eq!(
                call(&mut context, "drawingBufferDirty", &[], ""),
                json!(true)
            );
            context.presented();
            // An equal-size Canvas assignment still resets/publishes its bitmap.
            call(&mut context, "resizeCanvas", &[4, 4], "");
            assert_eq!(
                call(&mut context, "drawingBufferDirty", &[], ""),
                json!(true)
            );
        }
    });
}

fn context(antialias: bool, preserve: bool, alpha: bool) -> WebGl {
    WebGl::new(
        4,
        4,
        Options {
            api: ApiVersion::Two,
            antialias,
            preserve,
            alpha,
            ..Options::default()
        },
    )
    .unwrap()
}

#[test]
fn presentation_implicitly_clears_even_with_rasterizer_discard_and_write_masks() {
    session::run_native_test(|| {
        for antialias in [false, true] {
            for alpha in [false, true] {
                let mut context = context(antialias, false, alpha);
                super::volume_copy_tests::clear(&mut context, [1., 0., 0., 1.]);
                // Resolve/cache before retirement, then ensure neither path can
                // return stale red pixels after the implicit native clear.
                assert!(
                    context
                        .surface
                        .snapshot()
                        .unwrap()
                        .chunks_exact(4)
                        .all(|p| p == [255, 0, 0, 255])
                );
                call(&mut context, "enable", &[DISCARD as i64], "");
                call(&mut context, "enable", &[gl::SCISSOR_TEST as i64], "");
                call(&mut context, "scissor", &[1, 1, 0, 0], "");
                call(&mut context, "colorMask", &[0, 0, 0, 0], "");
                call(&mut context, "depthMask", &[0], "");
                call(
                    &mut context,
                    "stencilMaskSeparate",
                    &[gl::FRONT as i64, 3],
                    "",
                );
                call(
                    &mut context,
                    "stencilMaskSeparate",
                    &[gl::BACK as i64, 5],
                    "",
                );
                context.presented();
                assert!(
                    context
                        .surface
                        .snapshot()
                        .unwrap()
                        .chunks_exact(4)
                        .all(|p| p == [0, 0, 0, if alpha { 0 } else { 255 }])
                );
                assert_eq!(
                    call(&mut context, "isEnabled", &[DISCARD as i64], ""),
                    json!(true)
                );
                assert_eq!(
                    call(&mut context, "isEnabled", &[gl::SCISSOR_TEST as i64], ""),
                    json!(true)
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
                assert_eq!(
                    call(
                        &mut context,
                        "getParameter",
                        &[gl::DEPTH_WRITEMASK as i64],
                        ""
                    ),
                    json!(false)
                );
                assert_eq!(
                    call(
                        &mut context,
                        "getParameter",
                        &[gl::STENCIL_WRITEMASK as i64],
                        ""
                    ),
                    json!(3)
                );
                assert_eq!(
                    call(
                        &mut context,
                        "getParameter",
                        &[gl::STENCIL_BACK_WRITEMASK as i64],
                        ""
                    ),
                    json!(5)
                );
                assert_eq!(
                    call(
                        &mut context,
                        "getParameter",
                        &[gl::COLOR_CLEAR_VALUE as i64],
                        ""
                    ),
                    json!([1., 0., 0., 1.])
                );
                assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
            }
        }
    });
}

#[test]
fn presentation_preserves_author_framebuffers_and_preserve_drawing_buffer_contract() {
    session::run_native_test(|| {
        for preserve in [false, true] {
            let mut context = context(false, preserve, true);
            super::volume_copy_tests::clear(&mut context, [1., 0., 0., 1.]);
            let source = super::array_copy_boundary_tests::framebuffer(&mut context, 0x8058);
            super::volume_copy_tests::clear(&mut context, [0., 1., 0., 1.]);
            let draw = call(&mut context, "createFramebuffer", &[], "")
                .as_i64()
                .unwrap();
            call(&mut context, "bindFramebuffer", &[DRAW as i64, draw], "");
            call(&mut context, "enable", &[DISCARD as i64], "");
            context.presented();
            let expected = if preserve { [255, 0, 0, 255] } else { [0; 4] };
            assert!(
                context
                    .surface
                    .snapshot()
                    .unwrap()
                    .chunks_exact(4)
                    .all(|p| p == expected)
            );
            assert_eq!(
                call(
                    &mut context,
                    "getParameter",
                    &[gl::FRAMEBUFFER_BINDING as i64],
                    ""
                ),
                json!(draw)
            );
            assert_eq!(
                call(&mut context, "getParameter", &[0x8caa], ""),
                json!(source)
            );
            assert_eq!(
                call(&mut context, "isEnabled", &[DISCARD as i64], ""),
                json!(true)
            );
            let read = context
                .read_pixels(
                    &Command {
                        op: "readPixels".into(),
                        i: vec![0, 0, 4, 4, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 64],
                        f: vec![],
                        text: String::new(),
                    },
                    None,
                )
                .unwrap();
            assert!(read.chunks_exact(4).all(|p| p == [0, 255, 0, 255]));
            call(&mut context, "bindFramebuffer", &[READ as i64, 0], "");
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}
