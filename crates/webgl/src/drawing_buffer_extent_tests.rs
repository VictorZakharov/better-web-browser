//! Canvas size negotiation is separate from strict texture/renderbuffer allocations.
use super::api_version_tests::call;
use super::*;

fn resize(context: &mut WebGl, width: u32, height: u32) -> Result<Value> {
    context.dispatch(
        &Command {
            op: "resizeCanvas".into(),
            i: vec![width.into(), height.into()],
            f: vec![],
            text: String::new(),
        },
        None,
    )
}

#[test]
fn drawing_buffer_extent_native_creation_reports_actual_storage_and_viewport() {
    session::run_native_test(|| {
        for api in [ApiVersion::One, ApiVersion::Two] {
            let mut contexts = BackendContexts::default();
            let id = contexts
                .create(
                    u32::MAX,
                    1,
                    &json!({"api": if api == ApiVersion::Two {"webgl2"} else {"webgl1"},
                        "antialias": api == ApiVersion::Two})
                    .to_string(),
                )
                .unwrap();
            assert_eq!(
                contexts.execute(id, r#"{"op":"drawingBufferSize"}"#, None),
                json!([4096, 1])
            );
            assert_eq!(
                contexts.execute(id, r#"{"op":"getParameter","i":[2978]}"#, None),
                json!([0, 0, 4096, 1])
            );
            let (width, height, pixels) = contexts.snapshot(id).unwrap();
            assert_eq!((width, height, pixels.len()), (4096, 1, 4096 * 4));
            assert!(pixels.iter().all(|value| *value == 0));
            let context = &contexts.contexts[&id];
            assert_eq!(
                context.resource_bytes,
                Surface::allocation_bytes(width, height, context.options).unwrap()
            );
        }
    });
}

#[test]
fn drawing_buffer_extent_native_resize_negotiates_overlap_without_changing_author_state() {
    session::run_native_test(|| {
        for antialias in [false, true] {
            let mut context = WebGl::new(
                8,
                8,
                Options {
                    api: ApiVersion::Two,
                    antialias,
                    ..Options::default()
                },
            )
            .unwrap();
            let original = context.resource_bytes;
            let per_pixel = Surface::allocation_bytes(1, 1, context.options).unwrap();
            context.resource_limit = original + 100 * per_pixel;
            call(&mut context, "viewport", &[1, 2, 3, 4], "");
            call(&mut context, "scissor", &[1, 1, 0, 0], "");
            call(&mut context, "enable", &[gl::SCISSOR_TEST as i64], "");
            call(&mut context, "colorMask", &[0, 0, 0, 0], "");
            resize(&mut context, 20, 20).unwrap();
            assert_eq!((context.surface.width, context.surface.height), (10, 10));
            assert_eq!(context.resource_bytes, 100 * per_pixel);
            assert_eq!(
                call(&mut context, "getParameter", &[gl::VIEWPORT as i64], ""),
                json!([1, 2, 3, 4])
            );
            assert_eq!(
                call(&mut context, "getParameter", &[gl::SCISSOR_BOX as i64], ""),
                json!([1, 1, 0, 0])
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
            assert!(
                context
                    .surface
                    .snapshot()
                    .unwrap()
                    .iter()
                    .all(|value| *value == 0)
            );
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}

#[test]
fn drawing_buffer_extent_exhausted_overlap_reuses_and_clears_smaller_existing_surface() {
    session::run_native_test(|| {
        let mut context = WebGl::new(4, 4, Options::default()).unwrap();
        super::volume_copy_tests::clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        let bytes = context.resource_bytes;
        context.resource_limit = bytes;
        let framebuffer = context.surface.framebuffer;
        resize(&mut context, 40, 40).unwrap();
        assert_eq!(context.surface.framebuffer, framebuffer);
        assert_eq!((context.surface.width, context.surface.height), (4, 4));
        assert_eq!(context.resource_bytes, bytes);
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .iter()
                .all(|value| *value == 0)
        );
        assert_eq!(resize(&mut context, 2, 2), Err(gl::OUT_OF_MEMORY));
        assert_eq!(context.surface.framebuffer, framebuffer);
        assert_eq!(context.resource_bytes, bytes);
    });
}

#[test]
fn drawing_buffer_extent_zero_axes_have_real_one_pixel_storage_and_same_size_clear() {
    session::run_native_test(|| {
        let mut context = WebGl::new(4, 4, Options::default()).unwrap();
        resize(&mut context, 0, 0).unwrap();
        assert_eq!((context.surface.width, context.surface.height), (1, 1));
        super::volume_copy_tests::clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(context.surface.snapshot().unwrap(), [0, 255, 0, 255]);
        let bytes = context.resource_bytes;
        context.resource_limit = bytes;
        resize(&mut context, 0, 0).unwrap();
        assert_eq!(context.resource_bytes, bytes);
        assert_eq!(context.surface.snapshot().unwrap(), [0; 4]);
    });
}
