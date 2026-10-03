//! Staged GLES3 block storage samples native mip levels and array layers.
use super::api_version_tests::{call, version_two};
use super::compressed_capabilities::Family;
use super::compressed_texture_tests::{block, enable, texture, upload};
use super::core_uniform_tests::{VERTEX, program};
use super::*;

pub(super) fn sample(context: &mut WebGl, expression: &str, declaration: &str) -> Vec<u8> {
    program(
        context,
        VERTEX,
        &format!(
            "#version 300 es\nprecision highp float;precision highp int;{declaration}\nout vec4 color;void main(){{color={expression};}}"
        ),
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    assert_eq!(call(context, "getError", &[], ""), json!(0));
    context.surface.snapshot().unwrap()
}

#[test]
fn compressed_core_immutable_mips_are_initialized_and_keep_their_format() {
    session::run_native_test(|| {
        let mut context = version_two();
        for family in Family::ALL {
            enable(&mut context, family);
            for format in family.formats() {
                let id = texture(&mut context, gl::TEXTURE_2D);
                upload(
                    &mut context,
                    "texStorage2D",
                    &[gl::TEXTURE_2D as i64, 3, format as i64, 4, 4],
                    None,
                )
                .unwrap();
                call(
                    &mut context,
                    "texParameteri",
                    &[
                        gl::TEXTURE_2D as i64,
                        gl::TEXTURE_MIN_FILTER as i64,
                        gl::NEAREST_MIPMAP_NEAREST as i64,
                    ],
                    "",
                );
                let object = context.objects.get(id, Kind::Texture).unwrap();
                assert_eq!(object.immutable_levels, 3);
                assert_eq!(object.core_images.len(), 3);
                assert!(
                    object
                        .core_images
                        .values()
                        .all(|image| image.internal == format)
                );
                for level in 0..3 {
                    let pixels = sample(
                        &mut context,
                        &format!("textureLod(t,vec2(0.5),float({level}))"),
                        "uniform sampler2D t;",
                    );
                    let alpha = if matches!(format, 0x83f0 | 0x8c4c | 0x8dbb..=0x8dbe) {
                        255
                    } else {
                        0
                    };
                    assert!(
                        pixels
                            .chunks_exact(4)
                            .all(|pixel| pixel == [0, 0, 0, alpha]),
                        "{format:#x}, level {level}: {pixels:?}"
                    );
                }
                upload(
                    &mut context,
                    "compressedTexSubImage2D",
                    &[gl::TEXTURE_2D as i64, 2, 0, 0, 1, 1, format as i64],
                    Some(&block(format, 0xf800)),
                )
                .unwrap();
                let pixels = sample(
                    &mut context,
                    "textureLod(t,vec2(0.5),2.0)",
                    "uniform sampler2D t;",
                );
                assert!(
                    pixels
                        .chunks_exact(4)
                        .all(|pixel| pixel[0] == 255 && pixel[3] == 255)
                );
                assert_eq!(
                    upload(
                        &mut context,
                        "compressedTexImage2D",
                        &[gl::TEXTURE_2D as i64, 0, format as i64, 4, 4, 0],
                        Some(&block(format, 0xf800))
                    ),
                    Err(gl::INVALID_OPERATION)
                );
                assert_eq!(
                    upload(
                        &mut context,
                        "generateMipmap",
                        &[gl::TEXTURE_2D as i64],
                        None
                    ),
                    Err(gl::INVALID_OPERATION)
                );
            }
        }
    });
}

#[test]
fn compressed_core_array_layers_decode_independently_and_updates_preserve_neighbors() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context, Family::S3tc);
        texture(&mut context, 0x8c1a);
        let red = block(0x83f1, 0xf800);
        let green = block(0x83f1, 0x07e0);
        upload(
            &mut context,
            "compressedTexImage3D",
            &[0x8c1a, 0, 0x83f1, 4, 4, 3, 0],
            Some(&[red.clone(), green.clone(), red.clone()].concat()),
        )
        .unwrap();
        for (layer, expected) in [
            (0, [255, 0, 0, 255]),
            (1, [0, 255, 0, 255]),
            (2, [255, 0, 0, 255]),
        ] {
            let pixels = sample(
                &mut context,
                &format!("texture(t,vec3(0.5,0.5,float({layer})))"),
                "uniform highp sampler2DArray t;",
            );
            assert!(pixels.chunks_exact(4).all(|pixel| pixel == expected));
        }
        upload(
            &mut context,
            "compressedTexSubImage3D",
            &[0x8c1a, 0, 0, 0, 1, 4, 4, 1, 0x83f1],
            Some(&red),
        )
        .unwrap();
        for layer in 0..3 {
            let pixels = sample(
                &mut context,
                &format!("texture(t,vec3(0.5,0.5,float({layer})))"),
                "uniform highp sampler2DArray t;",
            );
            assert!(
                pixels
                    .chunks_exact(4)
                    .all(|pixel| pixel == [255, 0, 0, 255])
            );
        }
    });
}

#[test]
fn compressed_core_immutable_array_mips_keep_all_layers_zero_initialized() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context, Family::S3tc);
        let id = texture(&mut context, 0x8c1a);
        call(
            &mut context,
            "texStorage3D",
            &[0x8c1a, 3, 0x83f1, 4, 4, 3],
            "",
        );
        call(
            &mut context,
            "texParameteri",
            &[
                0x8c1a,
                gl::TEXTURE_MIN_FILTER as i64,
                gl::NEAREST_MIPMAP_NEAREST as i64,
            ],
            "",
        );
        assert!(
            context
                .objects
                .get(id, Kind::Texture)
                .unwrap()
                .core_images
                .values()
                .all(|image| image.depth == 3)
        );
        for level in 0..3 {
            for layer in 0..3 {
                let pixels = sample(
                    &mut context,
                    &format!("textureLod(t,vec3(0.5,0.5,float({layer})),float({level}))"),
                    "uniform highp sampler2DArray t;",
                );
                assert!(pixels.iter().all(|byte| *byte == 0));
            }
        }
        upload(
            &mut context,
            "compressedTexSubImage3D",
            &[0x8c1a, 2, 0, 0, 1, 1, 1, 1, 0x83f1],
            Some(&block(0x83f1, 0x07e0)),
        )
        .unwrap();
        for (layer, expected) in [(0, [0, 0, 0, 0]), (1, [0, 255, 0, 255]), (2, [0, 0, 0, 0])] {
            let pixels = sample(
                &mut context,
                &format!("textureLod(t,vec3(0.5,0.5,float({layer})),2.0)"),
                "uniform highp sampler2DArray t;",
            );
            assert!(pixels.chunks_exact(4).all(|pixel| pixel == expected));
        }
    });
}

#[test]
fn compressed_core_cube_storage_allocates_every_face_and_mip() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context, Family::S3tc);
        let id = texture(&mut context, gl::TEXTURE_CUBE_MAP);
        call(
            &mut context,
            "texStorage2D",
            &[gl::TEXTURE_CUBE_MAP as i64, 3, 0x83f1, 4, 4],
            "",
        );
        call(
            &mut context,
            "texParameteri",
            &[
                gl::TEXTURE_CUBE_MAP as i64,
                gl::TEXTURE_MIN_FILTER as i64,
                gl::NEAREST_MIPMAP_NEAREST as i64,
            ],
            "",
        );
        assert_eq!(
            context
                .objects
                .get(id, Kind::Texture)
                .unwrap()
                .core_images
                .len(),
            18
        );
        let target = gl::TEXTURE_CUBE_MAP_POSITIVE_X;
        upload(
            &mut context,
            "compressedTexSubImage2D",
            &[target as i64, 2, 0, 0, 1, 1, 0x83f1],
            Some(&block(0x83f1, 0xf800)),
        )
        .unwrap();
        for (direction, expected) in [
            ("vec3(1,0,0)", [255, 0, 0, 255]),
            ("vec3(0,1,0)", [0, 0, 0, 0]),
        ] {
            let pixels = sample(
                &mut context,
                &format!("textureLod(t,{direction},2.0)"),
                "uniform samplerCube t;",
            );
            assert!(pixels.chunks_exact(4).all(|pixel| pixel == expected));
        }
    });
}
