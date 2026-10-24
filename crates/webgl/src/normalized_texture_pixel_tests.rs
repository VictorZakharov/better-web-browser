//! The extension's normalized values survive actual driver sampling and copies.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{texture, upload};
use super::core_uniform_tests::{VERTEX, program};
use super::normalized_texture_tests::{attach, enable, image, words};
use super::*;

fn green(context: &mut WebGl, sampler: &str, expression: &str, expected: &str) {
    call(context, "bindFramebuffer", &[gl::FRAMEBUFFER as i64, 0], "");
    program(
        context,
        VERTEX,
        &format!(
            "#version 300 es\nprecision highp float;uniform highp {sampler} tex;out vec4 color;void main(){{vec4 v={expression};color=all(lessThan(abs(v-({expected})),vec4(0.0001)))?vec4(0,1,0,1):vec4(1,0,0,1);}}"
        ),
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    let pixels = context.surface.snapshot().unwrap();
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 255, 0, 255]),
        "{sampler}: {expression}, expected {expected}; pixels {pixels:?}"
    );
}

#[test]
fn norm16_sampling_uses_signed_normalization_and_preserves_low_unsigned_bits() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context);
        call(
            &mut context,
            "pixelStorei",
            &[gl::UNPACK_ALIGNMENT as i64, 1],
            "",
        );
        for (internal, values, expected) in [
            (0x822a, vec![257], "vec4(257.0/65535.0,0,0,1)"),
            (
                0x822c,
                vec![1, 32769],
                "vec4(1.0/65535.0,32769.0/65535.0,0,1)",
            ),
            (
                0x8054,
                vec![1, 32769, 65534],
                "vec4(1.0/65535.0,32769.0/65535.0,65534.0/65535.0,1)",
            ),
            (
                0x805b,
                vec![1, 32769, 65534, 257],
                "vec4(1.0/65535.0,32769.0/65535.0,65534.0/65535.0,257.0/65535.0)",
            ),
            (0x8f98, vec![0x8000], "vec4(-1,0,0,1)"),
            (0x8f99, vec![0xc000, 0x7fff], "vec4(-16384.0/32767.0,1,0,1)"),
            (0x8f9a, vec![0x8000, 1, 0x7fff], "vec4(-1,1.0/32767.0,1,1)"),
            (
                0x8f9b,
                vec![0x8000, 1, 0x7fff, 0],
                "vec4(-1,1.0/32767.0,1,0)",
            ),
        ] {
            let id = image(&mut context, internal, Some(&words(&values)));
            green(
                &mut context,
                "sampler2D",
                "texelFetch(tex,ivec2(0),0)",
                expected,
            );
            call(&mut context, "deleteTexture", &[id as i64], "");
        }
    });
}

#[test]
fn norm16_linear_filtering_and_mipmap_generation_use_real_normalized_texels() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context);
        for internal in [0x805b, 0x8f9b] {
            let id = texture(&mut context, gl::TEXTURE_2D);
            let kind = if internal == 0x805b {
                gl::UNSIGNED_SHORT
            } else {
                gl::SHORT
            };
            let maximum = if internal == 0x805b { 65535 } else { 32767 };
            let bytes = words(&[
                0, 0, 0, maximum, maximum, maximum, maximum, maximum, 0, 0, 0, maximum, maximum,
                maximum, maximum, maximum,
            ]);
            upload(
                &mut context,
                "texImage2D",
                &[
                    gl::TEXTURE_2D as i64,
                    0,
                    internal,
                    2,
                    2,
                    0,
                    gl::RGBA as i64,
                    kind as i64,
                ],
                Some(&bytes),
            )
            .unwrap();
            for pname in [gl::TEXTURE_MIN_FILTER, gl::TEXTURE_MAG_FILTER] {
                call(
                    &mut context,
                    "texParameteri",
                    &[gl::TEXTURE_2D as i64, pname as i64, gl::LINEAR as i64],
                    "",
                );
            }
            green(
                &mut context,
                "sampler2D",
                "texture(tex,vec2(0.5))",
                "vec4(0.5,0.5,0.5,1)",
            );
            if internal == 0x8f9b {
                // GLES3 requires both color renderability and filterability;
                // signed-normalized storage is not color-renderable here.
                assert_eq!(
                    upload(
                        &mut context,
                        "generateMipmap",
                        &[gl::TEXTURE_2D as i64],
                        None
                    ),
                    Err(gl::INVALID_OPERATION)
                );
            } else {
                call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
                call(
                    &mut context,
                    "texParameteri",
                    &[gl::TEXTURE_2D as i64, gl::TEXTURE_MIN_FILTER as i64, 0x2703],
                    "",
                );
                green(
                    &mut context,
                    "sampler2D",
                    "texelFetch(tex,ivec2(0),1)",
                    "vec4(0.5,0.5,0.5,1)",
                );
            }
            call(&mut context, "deleteTexture", &[id as i64], "");
        }
    });
}

#[test]
fn norm16_copy_images_and_volume_layers_preserve_precision_and_missing_alpha() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context);
        let source = image(&mut context, 0x805b, Some(&words(&[1, 32769, 65534, 257])));
        let fb = attach(&mut context, source);
        for internal in [0x822a, 0x822c, 0x8054, 0x805b] {
            call(
                &mut context,
                "bindFramebuffer",
                &[gl::FRAMEBUFFER as i64, fb as i64],
                "",
            );
            let id = texture(&mut context, gl::TEXTURE_2D);
            call(
                &mut context,
                "copyTexImage2D",
                &[gl::TEXTURE_2D as i64, 0, internal, 0, 0, 1, 1, 0],
                "",
            );
            let expected = match internal {
                0x822a => "vec4(1.0/65535.0,0,0,1)",
                0x822c => "vec4(1.0/65535.0,32769.0/65535.0,0,1)",
                0x8054 => "vec4(1.0/65535.0,32769.0/65535.0,65534.0/65535.0,1)",
                _ => "vec4(1.0/65535.0,32769.0/65535.0,65534.0/65535.0,257.0/65535.0)",
            };
            green(
                &mut context,
                "sampler2D",
                "texelFetch(tex,ivec2(0),0)",
                expected,
            );
            call(&mut context, "deleteTexture", &[id as i64], "");
            for target in [texture_targets::VOLUME, texture_targets::ARRAY] {
                call(
                    &mut context,
                    "bindFramebuffer",
                    &[gl::FRAMEBUFFER as i64, fb as i64],
                    "",
                );
                let id = texture(&mut context, target);
                call(
                    &mut context,
                    "texStorage3D",
                    &[target as i64, 1, internal, 1, 1, 2],
                    "",
                );
                call(
                    &mut context,
                    "copyTexSubImage3D",
                    &[target as i64, 0, 0, 0, 1, 0, 0, 1, 1],
                    "",
                );
                let sampler = if target == texture_targets::VOLUME {
                    "sampler3D"
                } else {
                    "sampler2DArray"
                };
                green(
                    &mut context,
                    sampler,
                    "texelFetch(tex,ivec3(0,0,1),0)",
                    expected,
                );
                green(
                    &mut context,
                    sampler,
                    "texelFetch(tex,ivec3(0,0,0),0)",
                    if internal == 0x805b {
                        "vec4(0)"
                    } else {
                        "vec4(0,0,0,1)"
                    },
                );
                call(&mut context, "deleteTexture", &[id as i64], "");
            }
        }
    });
}

#[test]
fn norm16_renderbuffers_require_unsigned_renderability_and_resolve_real_samples() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = call(&mut context, "createRenderbuffer", &[], "")
            .as_i64()
            .unwrap();
        call(
            &mut context,
            "bindRenderbuffer",
            &[gl::RENDERBUFFER as i64, id],
            "",
        );
        for internal in normalized_textures::FORMATS {
            assert_eq!(
                upload(
                    &mut context,
                    "renderbufferStorage",
                    &[gl::RENDERBUFFER as i64, internal as i64, 1, 1],
                    None
                ),
                Err(gl::INVALID_ENUM)
            );
        }
        enable(&mut context);
        for internal in normalized_textures::FORMATS {
            let valid = normalized_textures::RENDERABLE.contains(&internal);
            let result = upload(
                &mut context,
                "renderbufferStorage",
                &[gl::RENDERBUFFER as i64, internal as i64, 1, 1],
                None,
            );
            if !valid {
                assert_eq!(result, Err(gl::INVALID_ENUM));
                continue;
            }
            result.unwrap();
            let counts = call(
                &mut context,
                "getInternalformatParameter",
                &[
                    gl::RENDERBUFFER as i64,
                    internal as i64,
                    multisample::SAMPLES as i64,
                ],
                "",
            );
            let count = counts.as_array().unwrap().last().unwrap().as_i64().unwrap();
            call(
                &mut context,
                "renderbufferStorageMultisample",
                &[gl::RENDERBUFFER as i64, count, internal as i64, 1, 1],
                "",
            );
            let fb = call(&mut context, "createFramebuffer", &[], "")
                .as_i64()
                .unwrap();
            call(
                &mut context,
                "bindFramebuffer",
                &[gl::FRAMEBUFFER as i64, fb],
                "",
            );
            call(
                &mut context,
                "framebufferRenderbuffer",
                &[
                    gl::FRAMEBUFFER as i64,
                    gl::COLOR_ATTACHMENT0 as i64,
                    gl::RENDERBUFFER as i64,
                    id,
                ],
                "",
            );
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
                        op: "clearBufferfv".into(),
                        i: vec![0x1800, 0],
                        f: vec![0.25, 0.5, 0.75, 1.],
                        text: String::new(),
                    },
                    None,
                )
                .unwrap();
            let target = image(&mut context, internal, None);
            let destination = attach(&mut context, target);
            call(
                &mut context,
                "bindFramebuffer",
                &[framebuffer_guard::READ as i64, fb],
                "",
            );
            call(
                &mut context,
                "blitFramebuffer",
                &[
                    0,
                    0,
                    1,
                    1,
                    0,
                    0,
                    1,
                    1,
                    gl::COLOR_BUFFER_BIT as i64,
                    gl::NEAREST as i64,
                ],
                "",
            );
            call(
                &mut context,
                "bindFramebuffer",
                &[gl::FRAMEBUFFER as i64, destination as i64],
                "",
            );
            let bytes = context
                .read_pixels(
                    &Command {
                        op: "readPixels".into(),
                        i: vec![0, 0, 1, 1, gl::RGBA as i64, gl::UNSIGNED_SHORT as i64, 8],
                        f: vec![],
                        text: String::new(),
                    },
                    None,
                )
                .unwrap();
            let values: Vec<_> = bytes
                .chunks_exact(2)
                .map(|word| u16::from_ne_bytes(word.try_into().unwrap()))
                .collect();
            assert!(values[0].abs_diff(16384) <= 1);
            if internal != 0x822a {
                assert!(values[1].abs_diff(32768) <= 1);
            }
            if internal == 0x805b {
                assert!(values[2].abs_diff(49151) <= 1);
            }
            assert_eq!(values[3], 65535);
        }
    });
}
