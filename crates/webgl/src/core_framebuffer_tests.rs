//! Separate read/draw targets select real images, never private framebuffer names.
use super::api_version_tests::{call, version_two};
use super::framebuffer_guard::{DRAW, READ, READ_BINDING};
use super::*;

fn framebuffer(context: &mut WebGl) -> (u32, u32) {
    let framebuffer = call(context, "createFramebuffer", &[], "")
        .as_u64()
        .unwrap() as u32;
    call(
        context,
        "bindFramebuffer",
        &[gl::FRAMEBUFFER as i64, framebuffer as i64],
        "",
    );
    let texture = call(context, "createTexture", &[], "").as_u64().unwrap() as u32;
    call(
        context,
        "bindTexture",
        &[gl::TEXTURE_2D as i64, texture as i64],
        "",
    );
    call(
        context,
        "texStorage2D",
        &[gl::TEXTURE_2D as i64, 1, 0x8058, 4, 4],
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
    assert_eq!(
        call(
            context,
            "checkFramebufferStatus",
            &[gl::FRAMEBUFFER as i64],
            ""
        ),
        json!(gl::FRAMEBUFFER_COMPLETE)
    );
    (framebuffer, texture)
}
fn clear(context: &mut WebGl, color: &[f64]) {
    context
        .dispatch(
            &Command {
                op: "clearColor".into(),
                i: vec![],
                f: color.into(),
                text: String::new(),
            },
            None,
        )
        .unwrap();
    call(context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
}
fn read(context: &mut WebGl) -> Vec<u8> {
    context
        .read_pixels(
            &Command {
                op: "readPixels".into(),
                i: vec![0, 0, 4, 4, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 64],
                f: vec![],
                text: String::new(),
            },
            None,
        )
        .unwrap()
}

#[test]
fn webgl2_independent_framebuffers_read_from_one_image_while_clearing_another() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (first, _) = framebuffer(&mut context);
        clear(&mut context, &[1., 0., 0., 1.]);
        let (second, _) = framebuffer(&mut context);
        clear(&mut context, &[0., 1., 0., 1.]);
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, first as i64],
            "",
        );
        assert_eq!(
            call(&mut context, "getParameter", &[READ_BINDING as i64], ""),
            json!(first)
        );
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::FRAMEBUFFER_BINDING as i64],
                ""
            ),
            json!(second)
        );
        assert!(
            read(&mut context)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        clear(&mut context, &[0., 0., 1., 1.]);
        assert!(
            read(&mut context)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, second as i64],
            "",
        );
        assert!(
            read(&mut context)
                .chunks_exact(4)
                .all(|p| p == [0, 0, 255, 255])
        );
        call(&mut context, "deleteFramebuffer", &[first as i64], "");
        assert_eq!(
            call(&mut context, "getParameter", &[READ_BINDING as i64], ""),
            json!(second)
        );
        call(&mut context, "deleteFramebuffer", &[second as i64], "");
        assert_eq!(
            call(&mut context, "getParameter", &[READ_BINDING as i64], ""),
            Value::Null
        );
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::FRAMEBUFFER_BINDING as i64],
                ""
            ),
            Value::Null
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_read_routes_are_framebuffer_local_and_default_none_survives_resize_capture() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (first, _) = framebuffer(&mut context);
        call(&mut context, "readBuffer", &[gl::NONE as i64], "");
        let (second, _) = framebuffer(&mut context);
        assert_eq!(
            call(&mut context, "getParameter", &[0x0c02], ""),
            json!(gl::COLOR_ATTACHMENT0)
        );
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, first as i64],
            "",
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x0c02], ""),
            json!(gl::NONE)
        );
        call(&mut context, "bindFramebuffer", &[READ as i64, 0], "");
        call(&mut context, "readBuffer", &[gl::NONE as i64], "");
        call(&mut context, "resize", &[8, 8], "");
        context.surface.snapshot().unwrap();
        assert_eq!(
            call(&mut context, "getParameter", &[0x0c02], ""),
            json!(gl::NONE)
        );
        let mut actual = 0;
        unsafe {
            gl::GetIntegerv(0x0c02, &mut actual);
        }
        assert_eq!(actual as u32, gl::NONE);
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::FRAMEBUFFER_BINDING as i64],
                ""
            ),
            json!(second)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_deletion_detaches_texture_from_both_bound_framebuffers() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (first, texture) = framebuffer(&mut context);
        let second = call(&mut context, "createFramebuffer", &[], "")
            .as_u64()
            .unwrap() as u32;
        call(
            &mut context,
            "bindFramebuffer",
            &[DRAW as i64, second as i64],
            "",
        );
        call(
            &mut context,
            "framebufferTexture2D",
            &[
                DRAW as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                gl::TEXTURE_2D as i64,
                texture as i64,
                0,
            ],
            "",
        );
        call(&mut context, "deleteTexture", &[texture as i64], "");
        for id in [first, second] {
            assert!(
                context
                    .objects
                    .get(id, Kind::Framebuffer)
                    .unwrap()
                    .framebuffer_attachments
                    .is_empty()
            );
        }
        for target in [READ, DRAW] {
            assert_eq!(
                call(&mut context, "checkFramebufferStatus", &[target as i64], ""),
                json!(gl::FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT)
            );
        }
        assert!(context.objects.get(texture, Kind::Texture).is_err());
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
