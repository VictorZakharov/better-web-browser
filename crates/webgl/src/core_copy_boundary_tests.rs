//! Copy transactions preserve read/draw routes, old storage and private state.
use super::api_version_tests::{call, version_two};
use super::array_copy_boundary_tests::framebuffer;
use super::compressed_texture_tests::{texture, upload};
use super::core_uniform_tests::{VERTEX, program};
use super::framebuffer_guard::{DRAW, READ};
use super::*;

fn copy(context: &mut WebGl, internal: u32, width: i64, height: i64) -> Result<Value> {
    upload(
        context,
        "copyTexImage2D",
        &[
            gl::TEXTURE_2D as i64,
            0,
            internal as i64,
            0,
            0,
            width,
            height,
            0,
        ],
        None,
    )
}

fn clear(context: &mut WebGl, color: [f64; 4]) {
    context
        .dispatch(
            &Command {
                op: "clearColor".into(),
                i: vec![],
                f: color.to_vec(),
                text: String::new(),
            },
            None,
        )
        .unwrap();
    call(context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
}

fn sample(context: &mut WebGl) -> Vec<u8> {
    call(context, "bindFramebuffer", &[gl::FRAMEBUFFER as i64, 0], "");
    program(
        context,
        VERTEX,
        "#version 300 es\nprecision highp float;uniform highp sampler2D tex;out vec4 color;void main(){color=texelFetch(tex,ivec2(0),0);}",
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    context.surface.snapshot().unwrap()
}

#[test]
fn webgl2_full_and_subcopies_read_the_read_target_not_an_incomplete_draw_target() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source = framebuffer(&mut context, 0x8058);
        clear(&mut context, [0., 1., 0., 0.]);
        let draw = call(&mut context, "createFramebuffer", &[], "")
            .as_i64()
            .unwrap();
        call(&mut context, "bindFramebuffer", &[DRAW as i64, draw], "");
        let destination = texture(&mut context, gl::TEXTURE_2D);
        copy(&mut context, 0x8051, 4, 4).unwrap();
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
        assert!(
            sample(&mut context)
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255])
        );
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, source as i64],
            "",
        );
        call(
            &mut context,
            "bindFramebuffer",
            &[DRAW as i64, source as i64],
            "",
        );
        clear(&mut context, [1., 0., 0., 0.]);
        call(&mut context, "bindFramebuffer", &[DRAW as i64, draw], "");
        call(
            &mut context,
            "bindTexture",
            &[gl::TEXTURE_2D as i64, destination as i64],
            "",
        );
        upload(
            &mut context,
            "copyTexSubImage2D",
            &[gl::TEXTURE_2D as i64, 0, 0, 0, 0, 0, 1, 1],
            None,
        )
        .unwrap();
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::FRAMEBUFFER_BINDING as i64],
                ""
            ),
            json!(draw)
        );
        assert!(
            sample(&mut context)
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
    });
}

#[test]
fn webgl2_failed_copies_preserve_image_metadata_pixels_and_budget() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source = framebuffer(&mut context, 0x8058);
        clear(&mut context, [0., 1., 0., 1.]);
        let destination = texture(&mut context, gl::TEXTURE_2D);
        copy(&mut context, 0x8051, 4, 4).unwrap();
        let charged = context.resource_bytes;
        for (internal, error) in [
            (0, gl::INVALID_ENUM),
            (0x8235, gl::INVALID_OPERATION),
            (0x81a5, gl::INVALID_OPERATION),
        ] {
            assert_eq!(copy(&mut context, internal, 2, 2), Err(error));
            assert_eq!(context.resource_bytes, charged);
            let image = context
                .objects
                .get(destination, Kind::Texture)
                .unwrap()
                .core_images[&(gl::TEXTURE_2D, 0)];
            assert_eq!((image.internal, image.width, image.height), (0x8051, 4, 4));
        }
        let limit = context.resource_limit;
        context.resource_limit = charged;
        assert_eq!(copy(&mut context, 0x8051, 4, 4), Err(gl::OUT_OF_MEMORY));
        context.resource_limit = limit;
        assert_eq!(context.resource_bytes, charged);
        assert!(
            sample(&mut context)
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255])
        );
        let charged = context.resource_bytes;
        call(
            &mut context,
            "bindFramebuffer",
            &[gl::FRAMEBUFFER as i64, source as i64],
            "",
        );
        call(
            &mut context,
            "bindTexture",
            &[gl::TEXTURE_2D as i64, destination as i64],
            "",
        );
        upload(
            &mut context,
            "copyTexSubImage2D",
            &[gl::TEXTURE_2D as i64, 0, 4, 0, 0, 0, 1, 1],
            None,
        )
        .unwrap_err();
        assert_eq!(context.resource_bytes, charged);
    });
}

#[test]
fn webgl2_private_rgb_copy_restores_pixel_buffers_and_extended_store_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        framebuffer(&mut context, 0x8058);
        clear(&mut context, [1., 0., 0., 0.]);
        texture(&mut context, gl::TEXTURE_2D);
        let mut bindings = Vec::new();
        for target in [core_buffers::PIXEL_PACK, core_buffers::PIXEL_UNPACK] {
            let buffer = call(&mut context, "createBuffer", &[], "")
                .as_i64()
                .unwrap();
            call(&mut context, "bindBuffer", &[target as i64, buffer], "");
            call(
                &mut context,
                "bufferData",
                &[target as i64, 64, gl::STATIC_DRAW as i64],
                "",
            );
            bindings.push(buffer);
        }
        let state = [
            (gl::PACK_ALIGNMENT, 8),
            (gl::UNPACK_ALIGNMENT, 8),
            (0x0d02, 7),
            (0x0d03, 3),
            (0x0d04, 2),
            (0x0cf2, 9),
            (0x0cf3, 2),
            (0x0cf4, 3),
            (0x806d, 2),
            (0x806e, 8),
        ];
        for (pname, value) in state {
            call(&mut context, "pixelStorei", &[pname as i64, value], "");
        }
        copy(&mut context, 0x8051, 4, 4).unwrap();
        for (pname, value) in state {
            assert_eq!(
                call(&mut context, "getParameter", &[pname as i64], ""),
                json!(value)
            );
        }
        for (pname, buffer) in [0x88ed, 0x88ef].into_iter().zip(bindings) {
            assert_eq!(
                call(&mut context, "getParameter", &[pname], ""),
                json!(buffer)
            );
        }
        assert!(
            sample(&mut context)
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
    });
}
