//! Resolve native multisample color storage into a texture without CPU synthesis.
use super::api_version_tests::{call, version_two};
use super::framebuffer_guard::{DRAW, READ};
use super::*;

pub(super) fn framebuffer(context: &mut WebGl, target: u32) -> u32 {
    let id = call(context, "createFramebuffer", &[], "")
        .as_u64()
        .unwrap() as u32;
    call(context, "bindFramebuffer", &[target as i64, id as i64], "");
    id
}
pub(super) fn renderbuffer(context: &mut WebGl, samples: i64) -> u32 {
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
        "renderbufferStorageMultisample",
        &[gl::RENDERBUFFER as i64, samples, 0x8058, 4, 4],
        "",
    );
    id
}
pub(super) fn attach(context: &mut WebGl, target: u32, id: u32) {
    call(
        context,
        "framebufferRenderbuffer",
        &[
            target as i64,
            gl::COLOR_ATTACHMENT0 as i64,
            gl::RENDERBUFFER as i64,
            id as i64,
        ],
        "",
    );
}
pub(super) fn pixels(context: &mut WebGl) -> Vec<u8> {
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
pub(super) fn color(context: &mut WebGl, value: [f64; 4]) {
    context
        .dispatch(
            &Command {
                op: "clearColor".into(),
                i: vec![],
                f: value.into(),
                text: String::new(),
            },
            None,
        )
        .unwrap();
    call(context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
}

#[test]
fn webgl2_four_sample_native_resolve_preserves_bindings_and_all_pixels() {
    session::run_native_test(|| {
        let mut context = version_two();
        let counts = call(
            &mut context,
            "getInternalformatParameter",
            &[gl::RENDERBUFFER as i64, 0x8058, 0x80a9],
            "",
        );
        assert!(
            counts
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_i64().unwrap() >= 4)
        );
        let source = framebuffer(&mut context, gl::FRAMEBUFFER);
        let buffer = renderbuffer(&mut context, 4);
        attach(&mut context, gl::FRAMEBUFFER, buffer);
        assert!(
            call(
                &mut context,
                "getRenderbufferParameter",
                &[gl::RENDERBUFFER as i64, 0x8cab],
                ""
            )
            .as_i64()
            .unwrap()
                >= 4
        );
        color(&mut context, [1., 0., 0., 1.]);
        let destination = framebuffer(&mut context, DRAW);
        let texture = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindTexture",
            &[gl::TEXTURE_2D as i64, texture as i64],
            "",
        );
        call(
            &mut context,
            "texStorage2D",
            &[gl::TEXTURE_2D as i64, 1, 0x8058, 4, 4],
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
        call(
            &mut context,
            "blitFramebuffer",
            &[
                0,
                0,
                4,
                4,
                0,
                0,
                4,
                4,
                gl::COLOR_BUFFER_BIT as i64,
                gl::NEAREST as i64,
            ],
            "",
        );
        assert_eq!(context.read_framebuffer, source);
        assert_eq!(context.framebuffer, destination);
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, destination as i64],
            "",
        );
        assert!(
            pixels(&mut context)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_multisample_invalid_updates_leave_real_storage_unchanged() {
    session::run_native_test(|| {
        let mut context = version_two();
        framebuffer(&mut context, gl::FRAMEBUFFER);
        let buffer = renderbuffer(&mut context, 0);
        attach(&mut context, gl::FRAMEBUFFER, buffer);
        color(&mut context, [0., 1., 0., 1.]);
        for (args, error) in [
            (
                [gl::RENDERBUFFER as i64, -1, 0x8058, 4, 4],
                gl::INVALID_VALUE,
            ),
            (
                [gl::RENDERBUFFER as i64, 1000, 0x8058, 4, 4],
                gl::INVALID_VALUE,
            ),
            (
                [gl::RENDERBUFFER as i64, 4, 0x84f9, 4, 4],
                gl::INVALID_OPERATION,
            ),
            (
                [gl::RENDERBUFFER as i64, 4, 0x8058, 4096, 4096],
                gl::OUT_OF_MEMORY,
            ),
        ] {
            let command = Command {
                op: "renderbufferStorageMultisample".into(),
                i: args.into(),
                f: vec![],
                text: String::new(),
            };
            assert_eq!(context.dispatch(&command, None), Err(error));
        }
        assert!(
            pixels(&mut context)
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
        let oversized = Command {
            op: "blitFramebuffer".into(),
            i: vec![
                i32::MIN as i64,
                0,
                i32::MAX as i64,
                4,
                0,
                0,
                4,
                4,
                gl::COLOR_BUFFER_BIT as i64,
                gl::NEAREST as i64,
            ],
            f: vec![],
            text: String::new(),
        };
        assert_eq!(context.dispatch(&oversized, None), Err(gl::INVALID_VALUE));
        assert!(
            pixels(&mut context)
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
    });
}
