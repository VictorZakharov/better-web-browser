//! GLES3.0/WebGL2 may attach an unallocated immutable mip; it is incomplete.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{texture, upload};
use super::framebuffer_guard::{DRAW, READ};
use super::texture_targets::{ARRAY, VOLUME};
use super::*;

fn storage(context: &mut WebGl, target: u32) -> u32 {
    let id = texture(context, target);
    let mut args = vec![target as i64, 2, 0x8058, 4, 4];
    let volume = matches!(target, VOLUME | ARRAY);
    if volume {
        args.push(4);
    }
    call(
        context,
        if volume {
            "texStorage3D"
        } else {
            "texStorage2D"
        },
        &args,
        "",
    );
    id
}

fn attach(
    context: &mut WebGl,
    target: u32,
    texture_target: u32,
    id: u32,
    level: i64,
) -> Result<Value> {
    if matches!(texture_target, VOLUME | ARRAY) {
        upload(
            context,
            "framebufferTextureLayer",
            &[
                target as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                id as i64,
                level,
                0,
            ],
            None,
        )
    } else {
        let face = if texture_target == gl::TEXTURE_CUBE_MAP {
            gl::TEXTURE_CUBE_MAP_POSITIVE_X
        } else {
            texture_target
        };
        upload(
            context,
            "framebufferTexture2D",
            &[
                target as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                face as i64,
                id as i64,
                level,
            ],
            None,
        )
    }
}

fn identity(context: &mut WebGl, target: u32, id: u32, level: i64) {
    for (pname, value) in [
        (gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE, json!(gl::TEXTURE)),
        (gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME, json!(id)),
        (gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL, json!(level)),
    ] {
        assert_eq!(
            call(
                context,
                "getFramebufferAttachmentParameter",
                &[target as i64, gl::COLOR_ATTACHMENT0 as i64, pname as i64],
                ""
            ),
            value
        );
    }
}

#[test]
fn unallocated_immutable_mips_are_incomplete_not_failed_attachment_mutations() {
    session::run_native_test(|| {
        for texture_target in [gl::TEXTURE_2D, gl::TEXTURE_CUBE_MAP, VOLUME, ARRAY] {
            let mut context = version_two();
            let id = storage(&mut context, texture_target);
            let fb = call(&mut context, "createFramebuffer", &[], "")
                .as_i64()
                .unwrap();
            call(
                &mut context,
                "bindFramebuffer",
                &[gl::FRAMEBUFFER as i64, fb],
                "",
            );
            attach(&mut context, gl::FRAMEBUFFER, texture_target, id, 0).unwrap();
            assert_eq!(
                context.core_framebuffer_status(DRAW),
                Ok(gl::FRAMEBUFFER_COMPLETE)
            );
            let resources = context.resource_bytes;
            for level in [2, 3, 12] {
                attach(&mut context, gl::FRAMEBUFFER, texture_target, id, level).unwrap();
                identity(&mut context, gl::FRAMEBUFFER, id, level);
                assert_eq!(
                    context.core_framebuffer_status(DRAW),
                    Ok(gl::FRAMEBUFFER_INCOMPLETE_ATTACHMENT)
                );
                assert_eq!(
                    context.core_framebuffer_status(READ),
                    Ok(gl::FRAMEBUFFER_INCOMPLETE_ATTACHMENT)
                );
                assert_eq!(context.resource_bytes, resources);
                assert_eq!(
                    attach(&mut context, gl::FRAMEBUFFER, texture_target, id, -1),
                    Err(gl::INVALID_VALUE)
                );
                identity(&mut context, gl::FRAMEBUFFER, id, level);
                assert_eq!(
                    upload(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], None),
                    Err(gl::INVALID_FRAMEBUFFER_OPERATION)
                );
                assert_eq!(
                    context.read_pixels(
                        &Command {
                            op: "readPixels".into(),
                            i: vec![0, 0, 1, 1, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 4],
                            f: vec![],
                            text: String::new()
                        },
                        None
                    ),
                    Err(gl::INVALID_FRAMEBUFFER_OPERATION)
                );
                attach(&mut context, gl::FRAMEBUFFER, texture_target, id, 0).unwrap();
                assert_eq!(
                    context.core_framebuffer_status(DRAW),
                    Ok(gl::FRAMEBUFFER_COMPLETE)
                );
            }
            super::volume_copy_tests::clear(&mut context, [1., 0., 0., 1.]);
            assert_eq!(
                context
                    .read_pixels(
                        &Command {
                            op: "readPixels".into(),
                            i: vec![0, 0, 1, 1, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 4],
                            f: vec![],
                            text: String::new()
                        },
                        None
                    )
                    .unwrap(),
                [255, 0, 0, 255]
            );
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}

#[test]
fn undefined_immutable_read_image_does_not_make_the_independent_draw_target_incomplete() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = storage(&mut context, gl::TEXTURE_2D);
        let fb = call(&mut context, "createFramebuffer", &[], "")
            .as_i64()
            .unwrap();
        call(&mut context, "bindFramebuffer", &[READ as i64, fb], "");
        attach(&mut context, READ, gl::TEXTURE_2D, id, 2).unwrap();
        identity(&mut context, READ, id, 2);
        assert_eq!(
            context.core_framebuffer_status(DRAW),
            Ok(gl::FRAMEBUFFER_COMPLETE)
        );
        assert_eq!(
            context.core_framebuffer_status(READ),
            Ok(gl::FRAMEBUFFER_INCOMPLETE_ATTACHMENT)
        );
        super::volume_copy_tests::clear(&mut context, [0., 1., 0., 1.]);
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
        // Detachment is different from an attached-but-undefined texture image.
        attach(&mut context, READ, gl::TEXTURE_2D, 0, 0).unwrap();
        assert_eq!(
            context.core_framebuffer_status(READ),
            Ok(gl::FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT)
        );
        assert_eq!(
            call(
                &mut context,
                "getFramebufferAttachmentParameter",
                &[
                    READ as i64,
                    gl::COLOR_ATTACHMENT0 as i64,
                    gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME as i64
                ],
                ""
            ),
            Value::Null
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
