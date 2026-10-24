//! TexImage's GLint internalformat does not share TexStorage's GLenum errors.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::upload;
use super::*;

#[test]
fn invalid_image_internal_formats_preserve_cpu_and_pbo_texture_definitions() {
    session::run_native_test(|| {
        let mut context = version_two();
        let unpack = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        for target in [
            gl::TEXTURE_2D,
            texture_targets::VOLUME,
            texture_targets::ARRAY,
        ] {
            let texture = call(&mut context, "createTexture", &[], "")
                .as_i64()
                .unwrap();
            call(&mut context, "bindTexture", &[target as i64, texture], "");
            let volume = target != gl::TEXTURE_2D;
            let op = if volume { "texImage3D" } else { "texImage2D" };
            let mut args = vec![target as i64, 0, 0x8058, 2, 2];
            if volume {
                args.push(2);
            }
            args.extend([0, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64]);
            upload(&mut context, op, &args, None).unwrap();
            let baseline = context.resource_bytes;
            for pbo in [false, true] {
                call(
                    &mut context,
                    "bindBuffer",
                    &[
                        core_buffers::PIXEL_UNPACK as i64,
                        if pbo { unpack } else { 0 },
                    ],
                    "",
                );
                if pbo {
                    upload(
                        &mut context,
                        "bufferData",
                        &[
                            core_buffers::PIXEL_UNPACK as i64,
                            32,
                            gl::STATIC_DRAW as i64,
                        ],
                        Some(&[77; 32]),
                    )
                    .unwrap();
                }
                let charged = context.resource_bytes;
                for internal in [0xdead, 0x822a, core_texture_formats::RED] {
                    let mut invalid = args.clone();
                    invalid[2] = internal as i64;
                    if pbo {
                        invalid.push(0);
                    }
                    let operation = if pbo {
                        format!("{op}FromBuffer")
                    } else {
                        op.into()
                    };
                    assert_eq!(
                        upload(&mut context, &operation, &invalid, None),
                        Err(gl::INVALID_VALUE)
                    );
                    assert_eq!(context.resource_bytes, charged);
                    let image = context
                        .objects
                        .get(texture as u32, Kind::Texture)
                        .unwrap()
                        .core_images[&(target, 0)];
                    assert_eq!(
                        (image.internal, image.width, image.height, image.depth),
                        (0x8058, 2, 2, if volume { 2 } else { 1 })
                    );
                    assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
                }
                call(
                    &mut context,
                    "bindBuffer",
                    &[core_buffers::PIXEL_UNPACK as i64, 0],
                    "",
                );
            }
            // TexStorage uses a different parameter type and must keep ENUM.
            let storage_op = if volume {
                "texStorage3D"
            } else {
                "texStorage2D"
            };
            let mut storage_args = vec![target as i64, 1, 0xdead, 2, 2];
            if volume {
                storage_args.push(2);
            }
            assert_eq!(
                upload(&mut context, storage_op, &storage_args, None),
                Err(gl::INVALID_ENUM)
            );
            assert!(context.resource_bytes >= baseline);
            call(&mut context, "deleteTexture", &[texture], "");
        }
    });
}
