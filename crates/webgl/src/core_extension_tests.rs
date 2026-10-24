//! WebGL2 keeps core texture facilities separate from optional color admission.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{texture, upload};
use super::texture_capabilities::TextureCapability;
use super::*;

#[test]
fn webgl2_obsolete_texture_extension_names_are_not_advertised_or_admitted() {
    session::run_native_test(|| {
        let mut context = version_two();
        let names = call(&mut context, "supportedExtensions", &[], "");
        for capability in TextureCapability::ALL {
            if capability.exposed_in(ApiVersion::Two) {
                continue;
            }
            assert!(
                !names
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|name| name == capability.public_name())
            );
            assert_eq!(
                call(
                    &mut context,
                    "enableExtension",
                    &[],
                    capability.public_name()
                ),
                json!(false)
            );
            assert!(!context.extensions.textures.enabled(capability));
        }
        assert!(!context.extensions.core_color_float);
        // Core sized float storage is still available without an OES object.
        texture(&mut context, gl::TEXTURE_2D);
        call(
            &mut context,
            "texStorage2D",
            &[gl::TEXTURE_2D as i64, 1, 0x8814, 4, 4],
            "",
        );
    });
}

#[test]
fn webgl2_promoted_geometry_and_shader_contracts_do_not_return_legacy_extensions() {
    session::run_native_test(|| {
        let mut context = version_two();
        let names = call(&mut context, "supportedExtensions", &[], "");
        for name in [
            "WEBGL_draw_buffers",
            "ANGLE_instanced_arrays",
            "OES_vertex_array_object",
            "OES_element_index_uint",
            "OES_standard_derivatives",
            "EXT_frag_depth",
            "EXT_shader_texture_lod",
        ] {
            assert!(!names.as_array().unwrap().iter().any(|entry| entry == name));
            assert_eq!(
                call(&mut context, "enableExtension", &[], name),
                json!(false)
            );
        }
        assert!(context.extensions.draw_buffers);
        assert!(context.extensions.instancing);
        assert!(context.extensions.vertex_arrays);
        assert!(context.extensions.uint_indices);
        assert!(context.extensions.derivatives);
        assert!(context.extensions.frag_depth);
        assert!(context.extensions.texture_lod);
        // Rejecting obsolete object names must not disable their core shader
        // functionality, which is validated and executed by the GLES3 compiler.
        super::core_uniform_tests::program(
            &mut context,
            super::core_uniform_tests::VERTEX,
            "#version 300 es\nprecision highp float;out vec4 color;void main(){gl_FragDepth=0.5;color=vec4(dFdx(gl_FragCoord.x),dFdy(gl_FragCoord.y),1,1);}",
        );
        call(
            &mut context,
            "drawArrays",
            &[gl::TRIANGLES as i64, 0, 3],
            "",
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        let pixels = context.surface.snapshot().unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 255, 255, 255])
        );
    });
}

fn enable_half(context: &mut WebGl) {
    assert_eq!(
        call(
            context,
            "enableExtension",
            &[],
            "EXT_color_buffer_half_float"
        ),
        json!(true)
    );
    assert!(
        context
            .extensions
            .textures
            .enabled(TextureCapability::ColorHalfFloat)
    );
    assert!(
        !context
            .extensions
            .textures
            .enabled(TextureCapability::HalfFloat)
    );
    assert!(!context.extensions.core_color_float);
}

fn framebuffer(context: &mut WebGl, texture: u32) {
    let framebuffer = call(context, "createFramebuffer", &[], "")
        .as_u64()
        .unwrap();
    call(
        context,
        "bindFramebuffer",
        &[gl::FRAMEBUFFER as i64, framebuffer as i64],
        "",
    );
    call(
        context,
        "framebufferTexture2D",
        &[
            gl::FRAMEBUFFER as i64,
            gl::COLOR_ATTACHMENT0 as i64,
            gl::TEXTURE_2D as i64,
            texture as i64,
            0,
        ],
        "",
    );
}

fn read(context: &mut WebGl) -> Vec<f32> {
    let bytes = context
        .read_pixels(
            &Command {
                op: "readPixels".into(),
                i: vec![0, 0, 1, 1, gl::RGBA as i64, gl::FLOAT as i64, 16],
                f: vec![],
                text: String::new(),
            },
            None,
        )
        .unwrap();
    bytes
        .chunks_exact(4)
        .map(|value| f32::from_ne_bytes(value.try_into().unwrap()))
        .collect()
}

#[test]
fn webgl2_half_color_extension_admits_only_its_three_renderbuffer_formats() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = call(&mut context, "createRenderbuffer", &[], "")
            .as_u64()
            .unwrap() as i64;
        call(
            &mut context,
            "bindRenderbuffer",
            &[gl::RENDERBUFFER as i64, id],
            "",
        );
        let formats = [0x822d, 0x822f, 0x881a];
        for format in formats {
            assert_eq!(
                upload(
                    &mut context,
                    "renderbufferStorage",
                    &[gl::RENDERBUFFER as i64, format, 4, 4],
                    None
                ),
                Err(gl::INVALID_ENUM)
            );
        }
        enable_half(&mut context);
        for format in formats {
            call(
                &mut context,
                "renderbufferStorage",
                &[gl::RENDERBUFFER as i64, format, 4, 4],
                "",
            );
            assert_eq!(
                call(
                    &mut context,
                    "getRenderbufferParameter",
                    &[
                        gl::RENDERBUFFER as i64,
                        gl::RENDERBUFFER_INTERNAL_FORMAT as i64
                    ],
                    ""
                ),
                json!(format)
            );
        }
        for format in [0x881b, 0x8814, 0x822e, 0x8230, 0x8c3a] {
            assert_eq!(
                upload(
                    &mut context,
                    "renderbufferStorage",
                    &[gl::RENDERBUFFER as i64, format, 4, 4],
                    None
                ),
                Err(gl::INVALID_ENUM)
            );
            assert_eq!(
                context
                    .objects
                    .get(id as u32, Kind::Renderbuffer)
                    .unwrap()
                    .renderbuffer_format,
                0x881a
            );
        }
    });
}

#[test]
fn webgl2_half_color_targets_keep_hdr_values_and_missing_channel_defaults() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable_half(&mut context);
        for (format, expected) in [
            (0x822d, [2.25, 0., 0., 1.]),
            (0x822f, [2.25, -1., 0., 1.]),
            (0x881a, [2.25, -1., 0.5, 0.25]),
        ] {
            let id = texture(&mut context, gl::TEXTURE_2D);
            call(
                &mut context,
                "texStorage2D",
                &[gl::TEXTURE_2D as i64, 1, format, 4, 4],
                "",
            );
            framebuffer(&mut context, id);
            assert_eq!(
                call(
                    &mut context,
                    "checkFramebufferStatus",
                    &[gl::FRAMEBUFFER as i64],
                    ""
                ),
                json!(gl::FRAMEBUFFER_COMPLETE)
            );
            context
                .dispatch(
                    &Command {
                        op: "clearColor".into(),
                        i: vec![],
                        f: vec![2.25, -1., 0.5, 0.25],
                        text: String::new(),
                    },
                    None,
                )
                .unwrap();
            call(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
            assert_eq!(read(&mut context), expected);
            assert_eq!(
                call(
                    &mut context,
                    "getFramebufferAttachmentParameter",
                    &[gl::FRAMEBUFFER as i64, gl::COLOR_ATTACHMENT0 as i64, 0x8211],
                    ""
                ),
                json!(gl::FLOAT)
            );
        }
    });
}

#[test]
fn webgl2_half_color_extension_does_not_make_rgb_or_full_float_textures_renderable() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable_half(&mut context);
        for format in [0x881b, 0x8814, 0x822e, 0x8230, 0x8c3a] {
            let id = texture(&mut context, gl::TEXTURE_2D);
            call(
                &mut context,
                "texStorage2D",
                &[gl::TEXTURE_2D as i64, 1, format, 4, 4],
                "",
            );
            framebuffer(&mut context, id);
            assert_ne!(
                call(
                    &mut context,
                    "checkFramebufferStatus",
                    &[gl::FRAMEBUFFER as i64],
                    ""
                ),
                json!(gl::FRAMEBUFFER_COMPLETE)
            );
        }
    });
}
