//! WebGL2 aliasing overwrites both logical slots; layers identify distinct images.
use super::api_version_tests::{call, version_two};
use super::framebuffer_attachments::DEPTH_STENCIL_ATTACHMENT;
use super::framebuffer_guard::{DRAW, READ};
use super::*;

fn attached(context: &mut WebGl, point: u32) -> Value {
    call(
        context,
        "getFramebufferAttachmentParameter",
        &[
            gl::FRAMEBUFFER as i64,
            point as i64,
            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME as i64,
        ],
        "",
    )
}
fn renderbuffer(context: &mut WebGl, format: u32) -> u32 {
    let id = call(context, "createRenderbuffer", &[], "")
        .as_u64()
        .unwrap() as u32;
    call(
        context,
        "bindRenderbuffer",
        &[gl::RENDERBUFFER as i64, id as i64],
        "",
    );
    call(
        context,
        "renderbufferStorage",
        &[gl::RENDERBUFFER as i64, format as i64, 4, 4],
        "",
    );
    assert_eq!(
        call(
            context,
            "getRenderbufferParameter",
            &[
                gl::RENDERBUFFER as i64,
                gl::RENDERBUFFER_INTERNAL_FORMAT as i64
            ],
            ""
        ),
        json!(format)
    );
    id
}

#[test]
fn webgl2_depth_stencil_alias_overwrites_and_detaches_both_original_slots() {
    session::run_native_test(|| {
        let mut context = version_two();
        let framebuffer = call(&mut context, "createFramebuffer", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindFramebuffer",
            &[gl::FRAMEBUFFER as i64, framebuffer as i64],
            "",
        );
        let depth = renderbuffer(&mut context, gl::DEPTH_COMPONENT16);
        call(
            &mut context,
            "framebufferRenderbuffer",
            &[
                gl::FRAMEBUFFER as i64,
                gl::DEPTH_ATTACHMENT as i64,
                gl::RENDERBUFFER as i64,
                depth as i64,
            ],
            "",
        );
        assert_eq!(attached(&mut context, gl::DEPTH_ATTACHMENT), json!(depth));
        let combined = renderbuffer(&mut context, 0x88f0);
        call(
            &mut context,
            "framebufferRenderbuffer",
            &[
                gl::FRAMEBUFFER as i64,
                DEPTH_STENCIL_ATTACHMENT as i64,
                gl::RENDERBUFFER as i64,
                combined as i64,
            ],
            "",
        );
        for point in [
            gl::DEPTH_ATTACHMENT,
            gl::STENCIL_ATTACHMENT,
            DEPTH_STENCIL_ATTACHMENT,
        ] {
            assert_eq!(attached(&mut context, point), json!(combined));
        }
        call(
            &mut context,
            "framebufferRenderbuffer",
            &[
                gl::FRAMEBUFFER as i64,
                DEPTH_STENCIL_ATTACHMENT as i64,
                gl::RENDERBUFFER as i64,
                0,
            ],
            "",
        );
        for point in [
            gl::DEPTH_ATTACHMENT,
            gl::STENCIL_ATTACHMENT,
            DEPTH_STENCIL_ATTACHMENT,
        ] {
            assert_eq!(attached(&mut context, point), Value::Null);
        }
        assert!(context.objects.get(depth, Kind::Renderbuffer).is_ok());
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_layered_framebuffer_renders_only_selected_array_layer_and_reports_it() {
    session::run_native_test(|| {
        let mut context = version_two();
        let texture = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap();
        call(&mut context, "bindTexture", &[0x8c1a, texture as i64], "");
        call(
            &mut context,
            "texStorage3D",
            &[0x8c1a, 1, 0x8058, 4, 4, 2],
            "",
        );
        let framebuffer = call(&mut context, "createFramebuffer", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindFramebuffer",
            &[gl::FRAMEBUFFER as i64, framebuffer as i64],
            "",
        );
        call(
            &mut context,
            "framebufferTextureLayer",
            &[
                DRAW as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                texture as i64,
                0,
                1,
            ],
            "",
        );
        assert_eq!(
            call(
                &mut context,
                "getFramebufferAttachmentParameter",
                &[READ as i64, gl::COLOR_ATTACHMENT0 as i64, 0x8cd4],
                ""
            ),
            json!(1)
        );
        assert_eq!(
            call(
                &mut context,
                "getFramebufferAttachmentParameter",
                &[READ as i64, gl::COLOR_ATTACHMENT0 as i64, 0x8212],
                ""
            ),
            json!(8)
        );
        context
            .dispatch(
                &Command {
                    op: "clearColor".into(),
                    i: vec![],
                    f: vec![0., 0., 1., 1.],
                    text: String::new(),
                },
                None,
            )
            .unwrap();
        call(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
        let command = Command {
            op: "readPixels".into(),
            i: vec![0, 0, 4, 4, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 64],
            f: vec![],
            text: String::new(),
        };
        assert!(
            context
                .read_pixels(&command, None)
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [0, 0, 255, 255])
        );
        call(
            &mut context,
            "framebufferTextureLayer",
            &[
                READ as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                texture as i64,
                0,
                0,
            ],
            "",
        );
        let untouched = context.read_pixels(&command, None).unwrap();
        assert!(untouched.iter().all(|byte| *byte == 0), "{untouched:?}");
        call(
            &mut context,
            "framebufferTextureLayer",
            &[
                READ as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                texture as i64,
                0,
                1,
            ],
            "",
        );
        assert!(
            context
                .read_pixels(&command, None)
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [0, 0, 255, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
