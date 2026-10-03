//! Cross-check the closed table against actual GLES3 allocation and error rules.
use super::api_version_tests::{call, version_two};
use super::core_texture_formats as formats;
use super::*;

const SIZED: &[u32] = &[
    0x8229, 0x822b, 0x8051, 0x8c41, 0x8058, 0x8c43, 0x8f94, 0x8f95, 0x8f96, 0x8f97, 0x822d, 0x822f,
    0x881b, 0x881a, 0x822e, 0x8230, 0x8815, 0x8814, 0x8d62, 0x8056, 0x8057, 0x8059, 0x8c3a, 0x8c3d,
    0x8231, 0x8232, 0x8233, 0x8234, 0x8235, 0x8236, 0x8237, 0x8238, 0x8239, 0x823a, 0x823b, 0x823c,
    0x8d8f, 0x8d7d, 0x8d89, 0x8d77, 0x8d83, 0x8d71, 0x8d8e, 0x8d7c, 0x8d88, 0x8d76, 0x8d82, 0x8d70,
    0x906f, 0x81a5, 0x81a6, 0x8cac, 0x88f0, 0x8cad,
];

#[test]
fn webgl2_each_sized_format_allocates_native_storage_with_every_legal_upload_type() {
    session::run_native_test(|| {
        let mut context = version_two();
        for &internal in SIZED {
            let storage = formats::storage(internal).unwrap();
            let id = call(&mut context, "createTexture", &[], "")
                .as_u64()
                .unwrap();
            call(
                &mut context,
                "bindTexture",
                &[gl::TEXTURE_2D as i64, id as i64],
                "",
            );
            for &kind in storage.types {
                let (upload_bytes, storage_bytes) =
                    formats::upload(internal, storage.base, kind).unwrap();
                assert!(upload_bytes <= 16 && storage_bytes <= 16);
                let command = Command {
                    op: "texImage2D".into(),
                    i: vec![
                        gl::TEXTURE_2D as i64,
                        0,
                        internal as i64,
                        1,
                        1,
                        0,
                        storage.base as i64,
                        kind as i64,
                    ],
                    f: vec![],
                    text: String::new(),
                };
                assert_eq!(
                    context.dispatch(&command, Some(&[0; 16])),
                    Ok(Value::Null),
                    "internal {internal:x}, type {kind:x}"
                );
                assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
            }
            call(&mut context, "deleteTexture", &[id as i64], "");
        }
    });
}

#[test]
fn webgl2_texture_table_does_not_admit_unknown_enums_or_mismatched_signedness() {
    for (internal, base, kind, expected) in [
        (0xdead, gl::RGBA, gl::UNSIGNED_BYTE, gl::INVALID_ENUM),
        (0x8058, 0xdead, gl::UNSIGNED_BYTE, gl::INVALID_ENUM),
        (0x8058, gl::RGBA, 0xdead, gl::INVALID_ENUM),
        (
            0x8235,
            formats::RED_INTEGER,
            gl::UNSIGNED_INT,
            gl::INVALID_OPERATION,
        ),
        (
            0x8236,
            formats::RED,
            gl::UNSIGNED_INT,
            gl::INVALID_OPERATION,
        ),
        (0x8814, gl::RGBA, formats::HALF, gl::INVALID_OPERATION),
        (gl::RGBA, gl::RGBA, gl::FLOAT, gl::INVALID_OPERATION),
        (
            gl::LUMINANCE,
            gl::ALPHA,
            gl::UNSIGNED_BYTE,
            gl::INVALID_OPERATION,
        ),
        (0x88f0, 0x84f9, gl::UNSIGNED_INT, gl::INVALID_OPERATION),
    ] {
        assert_eq!(formats::upload(internal, base, kind), Err(expected));
    }
    assert_eq!(formats::upload(0x881a, gl::RGBA, gl::FLOAT), Ok((16, 8)));
    assert_eq!(formats::upload(0x8cad, 0x84f9, 0x8dad), Ok((8, 8)));
    assert_eq!(formats::upload(0x8057, gl::RGBA, 0x8368), Ok((4, 4)));
}

#[test]
fn webgl2_cube_immutable_storage_allocates_all_faces_and_mips() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap() as u32;
        call(
            &mut context,
            "bindTexture",
            &[gl::TEXTURE_CUBE_MAP as i64, id as i64],
            "",
        );
        call(
            &mut context,
            "texStorage2D",
            &[gl::TEXTURE_CUBE_MAP as i64, 3, 0x8058, 4, 4],
            "",
        );
        let object = context.objects.get(id, Kind::Texture).unwrap();
        assert_eq!(object.core_images.len(), 18);
        assert_eq!(object.capacity, 6 * (16 + 4 + 1) * 4);
        for target in 0x8515..=0x851a {
            for level in 0..3 {
                let image = object.core_images[&(target, level)];
                assert_eq!(image.width, 4 >> level);
                assert_eq!(image.height, 4 >> level);
            }
        }
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
