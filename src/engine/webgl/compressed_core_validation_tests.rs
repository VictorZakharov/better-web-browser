//! Staged immutable and array validation must leave native storage unchanged.
use super::api_version_tests::{call, version_two};
use super::compressed_capabilities::Family;
use super::compressed_core_tests::sample;
use super::compressed_texture_tests::{block, enable, texture, upload};
use super::*;

const ARRAY: i64 = 0x8c1a;

#[test]
fn compressed_array_invalid_definitions_never_allocate_or_replace_images() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context, Family::S3tc);
        let id = texture(&mut context, ARRAY as u32);
        let args = [ARRAY, 0, 0x83f1, 4, 4, 2, 0];
        let bytes = block(0x83f1, 0xf800).repeat(2);
        let baseline = context.resource_bytes;
        for (index, value, error) in [
            (0, gl::TEXTURE_2D as i64, gl::INVALID_ENUM),
            (0, 0x806f, gl::INVALID_OPERATION),
            (1, -1, gl::INVALID_VALUE),
            (1, 13, gl::INVALID_VALUE),
            (2, gl::RGBA as i64, gl::INVALID_ENUM),
            (3, -1, gl::INVALID_VALUE),
            (3, 4097, gl::INVALID_VALUE),
            (4, -1, gl::INVALID_VALUE),
            (5, -1, gl::INVALID_VALUE),
            (5, 257, gl::INVALID_VALUE),
            (6, 1, gl::INVALID_VALUE),
        ] {
            let mut invalid = args;
            invalid[index] = value;
            assert_eq!(
                upload(&mut context, "compressedTexImage3D", &invalid, Some(&bytes)),
                Err(error),
                "argument {index}={value}"
            );
            assert_eq!(context.resource_bytes, baseline);
            assert!(
                context
                    .objects
                    .get(id, Kind::Texture)
                    .unwrap()
                    .core_images
                    .is_empty()
            );
        }
        for length in [0, 7, 8, 15, 17, 32] {
            assert_eq!(
                upload(
                    &mut context,
                    "compressedTexImage3D",
                    &args,
                    Some(&vec![0; length])
                ),
                Err(gl::INVALID_VALUE)
            );
        }
        upload(&mut context, "compressedTexImage3D", &args, Some(&bytes)).unwrap();
        let charged = context.resource_bytes;
        assert_eq!(
            upload(&mut context, "compressedTexImage3D", &args, None),
            Err(gl::INVALID_VALUE)
        );
        assert_eq!(context.resource_bytes, charged);
        let pixels = sample(
            &mut context,
            "texture(t,vec3(0.5,0.5,1))",
            "uniform highp sampler2DArray t;",
        );
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
    });
}

#[test]
fn compressed_array_updates_check_layer_block_and_format_before_native_write() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context, Family::S3tc);
        texture(&mut context, ARRAY as u32);
        let red = block(0x83f1, 0xf800);
        let green = block(0x83f1, 0x07e0);
        upload(
            &mut context,
            "compressedTexImage3D",
            &[ARRAY, 0, 0x83f1, 8, 8, 2, 0],
            Some(&red.repeat(8)),
        )
        .unwrap();
        let args = [ARRAY, 0, 0, 0, 0, 4, 4, 1, 0x83f1];
        let charged = context.resource_bytes;
        for (index, value, error) in [
            (1, 1, gl::INVALID_OPERATION),
            (2, -1, gl::INVALID_VALUE),
            (2, 1, gl::INVALID_OPERATION),
            (2, 8, gl::INVALID_VALUE),
            (3, -1, gl::INVALID_VALUE),
            (3, 1, gl::INVALID_OPERATION),
            (4, -1, gl::INVALID_VALUE),
            (4, 2, gl::INVALID_VALUE),
            (5, 2, gl::INVALID_OPERATION),
            (6, 2, gl::INVALID_OPERATION),
            (8, 0x83f0, gl::INVALID_OPERATION),
            (8, 0x83f2, gl::INVALID_OPERATION),
        ] {
            let mut invalid = args;
            invalid[index] = value;
            assert_eq!(
                upload(
                    &mut context,
                    "compressedTexSubImage3D",
                    &invalid,
                    Some(&green)
                ),
                Err(error),
                "argument {index}={value}"
            );
            assert_eq!(context.resource_bytes, charged);
        }
        for layer in 0..2 {
            let pixels = sample(
                &mut context,
                &format!("texture(t,vec3(0.5,0.5,{layer}))"),
                "uniform highp sampler2DArray t;",
            );
            assert!(
                pixels
                    .chunks_exact(4)
                    .all(|pixel| pixel == [255, 0, 0, 255])
            );
        }
        upload(&mut context, "compressedTexSubImage3D", &args, Some(&green)).unwrap();
        let pixels = sample(
            &mut context,
            "texture(t,vec3(0.25,0.25,0))",
            "uniform highp sampler2DArray t;",
        );
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255])
        );
    });
}

#[test]
fn compressed_immutable_invalid_storage_is_atomic_and_extension_gated() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = texture(&mut context, gl::TEXTURE_2D);
        let args = [gl::TEXTURE_2D as i64, 3, 0x83f1, 4, 4];
        assert_eq!(
            upload(&mut context, "texStorage2D", &args, None),
            Err(gl::INVALID_ENUM)
        );
        enable(&mut context, Family::S3tc);
        let baseline = context.resource_bytes;
        for (index, value, error) in [
            (0, 0x8515, gl::INVALID_ENUM),
            (1, 0, gl::INVALID_VALUE),
            (1, 4, gl::INVALID_OPERATION),
            (2, 0x8c4d, gl::INVALID_ENUM),
            (3, 0, gl::INVALID_VALUE),
            (3, 4097, gl::INVALID_VALUE),
            (4, 0, gl::INVALID_VALUE),
        ] {
            let mut invalid = args;
            invalid[index] = value;
            assert_eq!(
                upload(&mut context, "texStorage2D", &invalid, None),
                Err(error),
                "argument {index}={value}"
            );
            assert_eq!(context.resource_bytes, baseline);
            let object = context.objects.get(id, Kind::Texture).unwrap();
            assert_eq!(object.immutable_levels, 0);
            assert!(object.core_images.is_empty());
        }
        call(&mut context, "texStorage2D", &args, "");
        assert_eq!(
            context
                .objects
                .get(id, Kind::Texture)
                .unwrap()
                .immutable_levels,
            3
        );
    });
}

#[test]
fn compressed_all_array_formats_initialize_every_layer_and_mip_transparently() {
    session::run_native_test(|| {
        let mut context = version_two();
        for family in Family::ALL {
            enable(&mut context, family);
            for format in family.formats() {
                texture(&mut context, ARRAY as u32);
                call(
                    &mut context,
                    "texStorage3D",
                    &[ARRAY, 3, format as i64, 4, 4, 3],
                    "",
                );
                call(
                    &mut context,
                    "texParameteri",
                    &[
                        ARRAY,
                        gl::TEXTURE_MIN_FILTER as i64,
                        gl::NEAREST_MIPMAP_NEAREST as i64,
                    ],
                    "",
                );
                let alpha = if matches!(format, 0x83f0 | 0x8c4c | 0x8dbb..=0x8dbe) {
                    255
                } else {
                    0
                };
                for level in 0..3 {
                    for layer in 0..3 {
                        let pixels = sample(
                            &mut context,
                            &format!("textureLod(t,vec3(0.5,0.5,{layer}),float({level}))"),
                            "uniform highp sampler2DArray t;",
                        );
                        assert!(
                            pixels
                                .chunks_exact(4)
                                .all(|pixel| pixel == [0, 0, 0, alpha]),
                            "format {format:#x}, layer {layer}, level {level}: {pixels:?}"
                        );
                    }
                }
            }
        }
    });
}
