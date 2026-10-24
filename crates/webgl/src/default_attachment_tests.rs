//! Logical default attachment queries must never leak private native identities.
use super::api_version_tests::call;
use super::compressed_texture_tests::upload;
use super::framebuffer_guard::{DRAW, READ};
use super::*;
fn query(context: &mut WebGl, target: u32, point: u32, pname: u32) -> Result<Value> {
    upload(
        context,
        "getFramebufferAttachmentParameter",
        &[target as i64, point as i64, pname as i64],
        None,
    )
}
fn context(alpha: bool, depth: bool, stencil: bool) -> WebGl {
    WebGl::new(
        4,
        4,
        Options {
            api: ApiVersion::Two,
            alpha,
            depth,
            stencil,
            ..Options::default()
        },
    )
    .unwrap()
}

#[test]
fn webgl2_default_attachment_component_sizes_match_granted_storage() {
    session::run_native_test(|| {
        for alpha in [false, true] {
            for depth in [false, true] {
                for stencil in [false, true] {
                    let mut context = context(alpha, depth, stencil);
                    for target in [gl::FRAMEBUFFER, DRAW, READ] {
                        assert_eq!(
                            query(
                                &mut context,
                                target,
                                gl::BACK,
                                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE
                            ),
                            Ok(json!(0x8218))
                        );
                        for pname in [0x8212, 0x8213, 0x8214] {
                            assert_eq!(query(&mut context, target, gl::BACK, pname), Ok(json!(8)));
                        }
                        assert_eq!(
                            query(&mut context, target, gl::BACK, 0x8215),
                            Ok(json!(if alpha { 8 } else { 0 }))
                        );
                        for pname in [0x8216, 0x8217] {
                            assert_eq!(query(&mut context, target, gl::BACK, pname), Ok(json!(0)));
                        }
                        assert_eq!(
                            query(&mut context, target, gl::BACK, 0x8210),
                            Ok(json!(gl::LINEAR))
                        );
                        assert_eq!(
                            query(&mut context, target, gl::BACK, 0x8211),
                            Ok(json!(0x8c17))
                        );
                        for (point, exists, pname, bits) in
                            [(0x1801, depth, 0x8216, 24), (0x1802, stencil, 0x8217, 8)]
                        {
                            assert_eq!(
                                query(
                                    &mut context,
                                    target,
                                    point,
                                    gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE
                                ),
                                Ok(json!(if exists { 0x8218 } else { 0 }))
                            );
                            if exists {
                                assert_eq!(
                                    query(&mut context, target, point, pname),
                                    Ok(json!(bits))
                                );
                            } else {
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
                                    query(&mut context, target, point, pname),
                                    Err(gl::INVALID_OPERATION)
                                );
                            }
                        }
                    }
                    assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
                }
            }
        }
    });
}

#[test]
fn webgl2_default_attachment_names_and_texture_fields_are_not_exposed() {
    session::run_native_test(|| {
        let mut context = context(true, true, true);
        for point in [gl::BACK, 0x1801, 0x1802] {
            for pname in [
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME,
                gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL,
                gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE,
                0x8cd4,
            ] {
                assert_eq!(
                    query(&mut context, gl::FRAMEBUFFER, point, pname),
                    Err(gl::INVALID_ENUM)
                );
            }
        }
        for point in [
            gl::COLOR_ATTACHMENT0,
            gl::DEPTH_ATTACHMENT,
            gl::STENCIL_ATTACHMENT,
            0x821a,
        ] {
            assert_eq!(
                query(
                    &mut context,
                    gl::FRAMEBUFFER,
                    point,
                    gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE
                ),
                Err(gl::INVALID_OPERATION)
            );
        }
        assert_eq!(
            query(
                &mut context,
                gl::FRAMEBUFFER,
                0xdead,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE
            ),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            query(&mut context, gl::FRAMEBUFFER, gl::BACK, 0xdead),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            query(
                &mut context,
                gl::TEXTURE_2D,
                gl::BACK,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE
            ),
            Err(gl::INVALID_ENUM)
        );
    });
}

#[test]
fn webgl2_default_read_queries_preserve_independent_author_draw_bindings() {
    session::run_native_test(|| {
        let mut context = context(true, true, false);
        let framebuffer = call(&mut context, "createFramebuffer", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindFramebuffer",
            &[DRAW as i64, framebuffer as i64],
            "",
        );
        assert_eq!(query(&mut context, READ, gl::BACK, 0x8212), Ok(json!(8)));
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::FRAMEBUFFER_BINDING as i64],
                ""
            ),
            json!(framebuffer)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8caa], ""),
            Value::Null
        );
        assert_eq!(
            query(&mut context, DRAW, gl::BACK, 0x8212),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            query(
                &mut context,
                DRAW,
                gl::COLOR_ATTACHMENT0,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE
            ),
            Ok(json!(gl::NONE))
        );
        assert_eq!(
            query(
                &mut context,
                DRAW,
                gl::COLOR_ATTACHMENT0,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME
            ),
            Ok(Value::Null)
        );
        call(&mut context, "readBuffer", &[gl::NONE as i64], "");
        assert_eq!(query(&mut context, READ, gl::BACK, 0x8212), Ok(json!(8)));
        assert_eq!(
            call(&mut context, "getParameter", &[0x0c02], ""),
            json!(gl::NONE)
        );
        call(&mut context, "bindFramebuffer", &[DRAW as i64, 0], "");
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, framebuffer as i64],
            "",
        );
        assert_eq!(query(&mut context, DRAW, gl::BACK, 0x8212), Ok(json!(8)));
        assert_eq!(
            query(&mut context, READ, gl::BACK, 0x8212),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8caa], ""),
            json!(framebuffer)
        );
    });
}

#[test]
fn webgl1_default_attachment_queries_keep_the_legacy_error_contract() {
    session::run_native_test(|| {
        let mut context = WebGl::new(4, 4, Options::default()).unwrap();
        assert_eq!(
            query(
                &mut context,
                gl::FRAMEBUFFER,
                gl::BACK,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE
            ),
            Err(gl::INVALID_OPERATION)
        );
    });
}
