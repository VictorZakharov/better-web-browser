//! Same-size canvas assignment clears the bitmap without temporary storage growth.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::context;
use super::*;

#[test]
fn unchanged_drawing_buffer_resize_clears_pixels_and_does_not_allocate_overlap() {
    session::run_native_test(|| {
        for factory in [context as fn() -> WebGl, version_two] {
            let mut context = factory();
            context
                .dispatch(
                    &Command {
                        op: "clearColor".into(),
                        i: vec![],
                        f: vec![1.0, 0.0, 0.0, 1.0],
                        text: String::new(),
                    },
                    None,
                )
                .unwrap();
            call(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
            assert!(
                context
                    .surface
                    .snapshot()
                    .unwrap()
                    .chunks_exact(4)
                    .all(|pixel| pixel == [255, 0, 0, 255])
            );
            let framebuffer = context.surface.framebuffer;
            let budget = context.resource_bytes;
            context.resource_limit = budget;
            call(&mut context, "colorMask", &[0, 0, 0, 0], "");
            call(&mut context, "scissor", &[0, 0, 1, 1], "");
            call(&mut context, "enable", &[gl::SCISSOR_TEST as i64], "");
            call(&mut context, "viewport", &[1, 2, 3, 4], "");
            for _ in 0..10 {
                call(&mut context, "resize", &[4, 4], "");
            }
            assert_eq!(context.resource_bytes, budget);
            assert_eq!(context.surface.framebuffer, framebuffer);
            assert!(
                context
                    .surface
                    .snapshot()
                    .unwrap()
                    .chunks_exact(4)
                    .all(|pixel| pixel == [0, 0, 0, 0])
            );
            assert_eq!(
                call(&mut context, "getParameter", &[gl::VIEWPORT as i64], ""),
                json!([1, 2, 3, 4])
            );
            assert_eq!(
                call(
                    &mut context,
                    "getParameter",
                    &[gl::COLOR_CLEAR_VALUE as i64],
                    ""
                ),
                json!([1.0, 0.0, 0.0, 1.0])
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
                call(&mut context, "isEnabled", &[gl::SCISSOR_TEST as i64], ""),
                json!(true)
            );
        }
    });
}

#[test]
fn unchanged_core_resize_preserves_both_author_framebuffer_bindings() {
    session::run_native_test(|| {
        let mut context = version_two();
        let read = call(&mut context, "createFramebuffer", &[], "")
            .as_u64()
            .unwrap();
        let draw = call(&mut context, "createFramebuffer", &[], "")
            .as_u64()
            .unwrap();
        call(&mut context, "bindFramebuffer", &[0x8ca8, read as i64], "");
        call(&mut context, "bindFramebuffer", &[0x8ca9, draw as i64], "");
        let budget = context.resource_bytes;
        context.resource_limit = budget;
        call(&mut context, "resize", &[4, 4], "");
        assert_eq!(
            call(&mut context, "getParameter", &[0x8caa], ""),
            json!(read)
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
        assert_eq!(context.resource_bytes, budget);
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 0, 0])
        );
    });
}
