//! GLES3 framebuffer error ordering, ignored detachment fields and object identity.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{texture, upload};
use super::framebuffer_attachments::DEPTH_STENCIL_ATTACHMENT;
use super::framebuffer_guard::{DRAW, READ};
use super::texture_targets::{ARRAY, VOLUME};
use super::*;

fn framebuffer(context: &mut WebGl, target: u32) -> u32 {
    let id = call(context, "createFramebuffer", &[], "")
        .as_u64()
        .unwrap() as u32;
    call(context, "bindFramebuffer", &[target as i64, id as i64], "");
    id
}

fn query(context: &mut WebGl, target: u32, point: u32, pname: u32) -> Result<Value> {
    upload(
        context,
        "getFramebufferAttachmentParameter",
        &[target as i64, point as i64, pname as i64],
        None,
    )
}

fn attachment(
    context: &mut WebGl,
    target: u32,
    point: u32,
    id: u32,
    level: i64,
    layer: i64,
) -> Result<Value> {
    upload(
        context,
        "framebufferTextureLayer",
        &[target as i64, point as i64, id as i64, level, layer],
        None,
    )
}

#[test]
fn framebuffer_contract_null_texture_detachment_ignores_all_image_fields() {
    session::run_native_test(|| {
        let mut context = version_two();
        for target in [gl::FRAMEBUFFER, DRAW, READ] {
            let id = framebuffer(&mut context, target);
            for texture_target in [gl::TEXTURE_2D, gl::TEXTURE_CUBE_MAP, VOLUME, ARRAY] {
                let tex = texture(&mut context, texture_target);
                for invalid in [-1, 1000, i32::MAX as i64] {
                    let layered = matches!(texture_target, VOLUME | ARRAY);
                    let face = if texture_target == gl::TEXTURE_CUBE_MAP {
                        gl::TEXTURE_CUBE_MAP_POSITIVE_X
                    } else {
                        texture_target
                    };
                    if layered {
                        attachment(&mut context, target, gl::COLOR_ATTACHMENT0, tex, 0, 0).unwrap();
                        attachment(
                            &mut context,
                            target,
                            gl::COLOR_ATTACHMENT0,
                            0,
                            invalid,
                            invalid,
                        )
                        .unwrap();
                    } else {
                        call(
                            &mut context,
                            "framebufferTexture2D",
                            &[
                                target as i64,
                                gl::COLOR_ATTACHMENT0 as i64,
                                face as i64,
                                tex as i64,
                                0,
                            ],
                            "",
                        );
                        call(
                            &mut context,
                            "framebufferTexture2D",
                            &[
                                target as i64,
                                gl::COLOR_ATTACHMENT0 as i64,
                                0xdead,
                                0,
                                invalid,
                            ],
                            "",
                        );
                    }
                    assert_eq!(
                        query(
                            &mut context,
                            target,
                            gl::COLOR_ATTACHMENT0,
                            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE
                        ),
                        Ok(json!(gl::NONE))
                    );
                    assert_eq!(
                        query(
                            &mut context,
                            target,
                            gl::COLOR_ATTACHMENT0,
                            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME
                        ),
                        Ok(Value::Null)
                    );
                    assert_eq!(
                        call(&mut context, "checkFramebufferStatus", &[target as i64], ""),
                        json!(gl::FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT)
                    );
                    assert!(
                        context
                            .objects
                            .get(id, Kind::Framebuffer)
                            .unwrap()
                            .framebuffer_attachments
                            .is_empty()
                    );
                    assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
                }
            }
            // Ignoring image fields does not ignore the attachment selector.
            assert_eq!(
                attachment(&mut context, target, 0xdead, 0, -1, -1),
                Err(gl::INVALID_ENUM)
            );
            assert_eq!(
                attachment(&mut context, target, gl::COLOR_ATTACHMENT0 + 31, 0, -1, -1),
                Err(gl::INVALID_OPERATION)
            );
        }
    });
}

#[test]
fn framebuffer_contract_invalid_read_selectors_preserve_independent_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        for (selector, error) in [
            (0xdead, gl::INVALID_ENUM),
            (gl::COLOR_ATTACHMENT0 - 1, gl::INVALID_ENUM),
            (gl::COLOR_ATTACHMENT0, gl::INVALID_OPERATION),
        ] {
            assert_eq!(
                upload(&mut context, "readBuffer", &[selector as i64], None),
                Err(error)
            );
            assert_eq!(
                call(&mut context, "getParameter", &[0x0c02], ""),
                json!(gl::BACK)
            );
        }
        let read = framebuffer(&mut context, READ);
        let draw = framebuffer(&mut context, DRAW);
        call(
            &mut context,
            "readBuffer",
            &[gl::COLOR_ATTACHMENT0 as i64 + 1],
            "",
        );
        for (selector, error) in [
            (0xdead, gl::INVALID_ENUM),
            (gl::BACK, gl::INVALID_OPERATION),
            (gl::COLOR_ATTACHMENT0 + 31, gl::INVALID_OPERATION),
        ] {
            assert_eq!(
                upload(&mut context, "readBuffer", &[selector as i64], None),
                Err(error)
            );
            assert_eq!(
                call(&mut context, "getParameter", &[0x0c02], ""),
                json!(gl::COLOR_ATTACHMENT0 + 1)
            );
            assert_eq!(context.read_framebuffer, read);
            assert_eq!(context.framebuffer, draw);
        }
        call(&mut context, "bindFramebuffer", &[READ as i64, 0], "");
        call(&mut context, "readBuffer", &[gl::NONE as i64], "");
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, read as i64],
            "",
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x0c02], ""),
            json!(gl::COLOR_ATTACHMENT0 + 1)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn framebuffer_contract_missing_image_error_precedes_invalid_query_name() {
    session::run_native_test(|| {
        let mut context = version_two();
        framebuffer(&mut context, READ);
        framebuffer(&mut context, DRAW);
        for target in [READ, DRAW, gl::FRAMEBUFFER] {
            for point in [
                gl::COLOR_ATTACHMENT0,
                gl::DEPTH_ATTACHMENT,
                gl::STENCIL_ATTACHMENT,
            ] {
                for pname in [
                    0,
                    0xdead,
                    0x8212,
                    0x8216,
                    gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL,
                ] {
                    assert_eq!(
                        query(&mut context, target, point, pname),
                        Err(gl::INVALID_OPERATION)
                    );
                }
                assert_eq!(
                    query(
                        &mut context,
                        target,
                        point,
                        gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME
                    ),
                    Ok(Value::Null)
                );
                assert_eq!(
                    query(
                        &mut context,
                        target,
                        point,
                        gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE
                    ),
                    Ok(json!(0))
                );
            }
        }
        let tex = texture(&mut context, gl::TEXTURE_2D);
        call(
            &mut context,
            "framebufferTexture2D",
            &[
                DRAW as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                gl::TEXTURE_2D as i64,
                tex as i64,
                0,
            ],
            "",
        );
        assert_eq!(
            query(&mut context, DRAW, gl::COLOR_ATTACHMENT0, 0),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            query(&mut context, READ, gl::COLOR_ATTACHMENT0, 0),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            query(
                &mut context,
                DRAW,
                gl::COLOR_ATTACHMENT0 + 31,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME
            ),
            Err(gl::INVALID_OPERATION)
        );
    });
}

#[test]
fn framebuffer_contract_depth_stencil_query_compares_objects_not_layer_images() {
    session::run_native_test(|| {
        let mut context = version_two();
        framebuffer(&mut context, gl::FRAMEBUFFER);
        let tex = texture(&mut context, ARRAY);
        call(
            &mut context,
            "texStorage3D",
            &[ARRAY as i64, 1, 0x88f0, 4, 4, 2],
            "",
        );
        attachment(&mut context, DRAW, gl::DEPTH_ATTACHMENT, tex, 0, 0).unwrap();
        attachment(&mut context, DRAW, gl::STENCIL_ATTACHMENT, tex, 0, 1).unwrap();
        assert_eq!(
            call(&mut context, "checkFramebufferStatus", &[DRAW as i64], ""),
            json!(gl::FRAMEBUFFER_UNSUPPORTED)
        );
        for target in [READ, DRAW, gl::FRAMEBUFFER] {
            assert_eq!(
                query(
                    &mut context,
                    target,
                    DEPTH_STENCIL_ATTACHMENT,
                    gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME
                ),
                Ok(json!(tex))
            );
            assert_eq!(
                query(&mut context, target, DEPTH_STENCIL_ATTACHMENT, 0x8211),
                Err(gl::INVALID_OPERATION)
            );
        }
        let other = texture(&mut context, ARRAY);
        call(
            &mut context,
            "texStorage3D",
            &[ARRAY as i64, 1, 0x88f0, 4, 4, 2],
            "",
        );
        attachment(&mut context, DRAW, gl::STENCIL_ATTACHMENT, other, 0, 0).unwrap();
        assert_eq!(
            query(
                &mut context,
                DRAW,
                DEPTH_STENCIL_ATTACHMENT,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            query(
                &mut context,
                DRAW,
                gl::DEPTH_ATTACHMENT,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME
            ),
            Ok(json!(tex))
        );
        assert_eq!(
            query(
                &mut context,
                DRAW,
                gl::STENCIL_ATTACHMENT,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME
            ),
            Ok(json!(other))
        );
        attachment(&mut context, DRAW, gl::STENCIL_ATTACHMENT, tex, 0, 0).unwrap();
        assert_eq!(
            call(&mut context, "checkFramebufferStatus", &[DRAW as i64], ""),
            json!(gl::FRAMEBUFFER_COMPLETE)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
