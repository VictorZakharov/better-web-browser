//! Byte footprints, extension epochs, and block boundaries are frontend contracts.
use super::compressed_capabilities::Family;
use super::compressed_texture_tests::{block, context, enable, texture, upload};
use super::*;

#[test]
fn compressed_footprints_and_family_specific_mip_rules_are_checked() {
    for family in Family::ALL {
        for value in family.formats() {
            let format = compressed_formats::format(value).unwrap();
            assert_eq!(format.byte_size(0, 4), Ok(0));
            assert_eq!(format.byte_size(1, 1), Ok(format.block_bytes));
            assert_eq!(format.byte_size(5, 7), Ok(format.block_bytes * 4));
            assert_eq!(format.image_dimensions(4, 8, 0), Ok(()));
            assert_eq!(format.image_dimensions(2, 2, 0), Err(gl::INVALID_OPERATION));
            assert_eq!(format.image_dimensions(2, 2, 1), Ok(()));
            assert_eq!(format.image_dimensions(3, 3, 1), Err(gl::INVALID_OPERATION));
            assert_eq!(
                format.image_dimensions(3, 3, 2),
                if family == Family::Srgb {
                    Err(gl::INVALID_OPERATION)
                } else {
                    Ok(())
                }
            );
            assert_eq!(format.subregion((8, 8), (4, 4), (4, 4)), Ok(()));
            assert_eq!(
                format.subregion((8, 8), (1, 0), (4, 4)),
                Err(gl::INVALID_OPERATION)
            );
            assert_eq!(
                format.subregion((8, 8), (4, 0), (8, 4)),
                Err(gl::INVALID_VALUE)
            );
            assert_eq!(
                format.subregion((7, 7), (4, 4), (3, 3)),
                if family == Family::Rgtc {
                    Ok(())
                } else {
                    Err(gl::INVALID_OPERATION)
                }
            );
        }
    }
    assert!(matches!(
        compressed_formats::format(gl::RGBA),
        Err(gl::INVALID_ENUM)
    ));
}

#[test]
fn compressed_invalid_uploads_do_not_define_storage() {
    session::run_native_test(|| {
        let mut context = context();
        let id = texture(&mut context, gl::TEXTURE_2D);
        let args = [gl::TEXTURE_2D as i64, 0, 0x83f1, 4, 4, 0];
        let bytes = block(0x83f1, 0xf800);
        assert_eq!(
            upload(&mut context, "compressedTexImage2D", &args, Some(&bytes)),
            Err(gl::INVALID_ENUM)
        );
        enable(&mut context, Family::S3tc);
        for length in [0, 7, 9, 16] {
            assert_eq!(
                upload(
                    &mut context,
                    "compressedTexImage2D",
                    &args,
                    Some(&vec![0; length])
                ),
                Err(gl::INVALID_VALUE)
            );
        }
        assert_eq!(
            upload(&mut context, "compressedTexImage2D", &args, None),
            Err(gl::INVALID_VALUE)
        );
        for (index, value, error) in [
            (0, gl::TEXTURE_CUBE_MAP as i64, gl::INVALID_ENUM),
            (1, -1, gl::INVALID_VALUE),
            (1, 13, gl::INVALID_VALUE),
            (3, -1, gl::INVALID_VALUE),
            (3, 2, gl::INVALID_OPERATION),
            (5, 1, gl::INVALID_VALUE),
        ] {
            let mut invalid = args;
            invalid[index] = value;
            let expected_length = if index == 3 && value == 2 {
                8
            } else {
                bytes.len()
            };
            assert_eq!(
                upload(
                    &mut context,
                    "compressedTexImage2D",
                    &invalid,
                    Some(&vec![0; expected_length])
                ),
                Err(error)
            );
        }
        assert!(
            context
                .objects
                .get(id, Kind::Texture)
                .unwrap()
                .core_images
                .is_empty()
        );
        upload(&mut context, "compressedTexImage2D", &args, Some(&bytes)).unwrap();
        let capacity = context.objects.get(id, Kind::Texture).unwrap().capacity;
        for _ in 0..16 {
            upload(&mut context, "compressedTexImage2D", &args, Some(&bytes)).unwrap();
        }
        assert_eq!(
            context.objects.get(id, Kind::Texture).unwrap().capacity,
            capacity
        );
    });
}

#[test]
fn compressed_subimages_require_matching_allocated_blocks() {
    session::run_native_test(|| {
        let mut context = context();
        enable(&mut context, Family::S3tc);
        texture(&mut context, gl::TEXTURE_2D);
        let bytes = block(0x83f1, 0xf800);
        let args = [gl::TEXTURE_2D as i64, 0, 0, 0, 4, 4, 0x83f1];
        assert_eq!(
            upload(&mut context, "compressedTexSubImage2D", &args, Some(&bytes)),
            Err(gl::INVALID_OPERATION)
        );
        upload(
            &mut context,
            "compressedTexImage2D",
            &[gl::TEXTURE_2D as i64, 0, 0x83f1, 8, 8, 0],
            Some(&bytes.repeat(4)),
        )
        .unwrap();
        assert_eq!(
            upload(
                &mut context,
                "compressedTexSubImage2D",
                &[gl::TEXTURE_2D as i64, 0, 0, 0, 8, 8, 0x83f2],
                Some(&bytes.repeat(4))
            ),
            Err(gl::INVALID_OPERATION)
        );
        for (index, value, error) in [
            (2, -1, gl::INVALID_VALUE),
            (2, 1, gl::INVALID_OPERATION),
            (2, 8, gl::INVALID_VALUE),
            (4, 2, gl::INVALID_OPERATION),
            (6, 0x83f0, gl::INVALID_OPERATION),
        ] {
            let mut invalid = args;
            invalid[index] = value;
            assert_eq!(
                upload(
                    &mut context,
                    "compressedTexSubImage2D",
                    &invalid,
                    Some(&bytes)
                ),
                Err(error)
            );
        }
        assert_eq!(
            upload(
                &mut context,
                "generateMipmap",
                &[gl::TEXTURE_2D as i64],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
    });
}

#[test]
fn compressed_extension_permission_does_not_leak_between_contexts() {
    session::run_native_test(|| {
        let mut first = context();
        enable(&mut first, Family::S3tc);
        let mut second = context();
        texture(&mut second, gl::TEXTURE_2D);
        assert_eq!(
            upload(
                &mut second,
                "compressedTexImage2D",
                &[gl::TEXTURE_2D as i64, 0, 0x83f1, 4, 4, 0],
                Some(&block(0x83f1, 0xf800))
            ),
            Err(gl::INVALID_ENUM)
        );
    });
}
