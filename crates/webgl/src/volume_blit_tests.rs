//! Private 3D blits retain CopyTexSubImage validation and author state.
use super::api_version_tests::{call, version_two};
use super::array_copy_boundary_tests::framebuffer;
use super::compressed_texture_tests::upload;
use super::framebuffer_guard::READ;
use super::texture_targets::VOLUME;
use super::volume_copy_tests::{clear, sample, storage};
use super::*;

fn copy(context: &mut WebGl, source: [i64; 4]) -> Result<Value> {
    upload(
        context,
        "copyTexSubImage3D",
        &[
            VOLUME as i64,
            0,
            0,
            0,
            1,
            source[0],
            source[1],
            source[2],
            source[3],
        ],
        None,
    )
}

#[test]
fn webgl2_volume_blit_clips_source_and_ignores_scissor_and_write_masks() {
    session::run_native_test(|| {
        let mut context = version_two();
        storage(&mut context, VOLUME);
        clear(&mut context, [0., 1., 0., 1.]);
        call(&mut context, "enable", &[gl::SCISSOR_TEST as i64], "");
        call(&mut context, "scissor", &[0, 0, 0, 0], "");
        call(&mut context, "colorMask", &[0, 0, 0, 0], "");
        let charge = context.resource_bytes;
        copy(&mut context, [-2, 0, 4, 4]).unwrap();
        assert_eq!(context.resource_bytes, charge);
        assert_eq!(
            call(&mut context, "isEnabled", &[gl::SCISSOR_TEST as i64], ""),
            json!(true)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[gl::SCISSOR_BOX as i64], ""),
            json!([0, 0, 0, 0])
        );
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::COLOR_WRITEMASK as i64],
                ""
            ),
            json!([false, false, false, false])
        );
        call(&mut context, "disable", &[gl::SCISSOR_TEST as i64], "");
        call(&mut context, "colorMask", &[1, 1, 1, 1], "");
        for (uv, expected) in [
            ("vec3(0.25,0.5,0.5)", [0, 0, 0, 0]),
            ("vec3(0.75,0.5,0.5)", [0, 255, 0, 255]),
            ("vec3(0.75,0.5,0.1)", [0, 0, 0, 0]),
        ] {
            assert!(
                sample(&mut context, VOLUME, uv, 0)
                    .chunks_exact(4)
                    .all(|p| p == expected)
            );
        }
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_volume_blit_keeps_independent_read_and_draw_framebuffers() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source = framebuffer(&mut context, 0x8058);
        clear(&mut context, [1., 0., 0., 1.]);
        let draw = framebuffer(&mut context, 0x8058);
        clear(&mut context, [0., 0., 1., 1.]);
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, source as i64],
            "",
        );
        storage(&mut context, VOLUME);
        copy(&mut context, [0, 0, 4, 4]).unwrap();
        assert_eq!(
            call(&mut context, "getParameter", &[0x8caa], ""),
            json!(source)
        );
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::FRAMEBUFFER_BINDING as i64],
                ""
            ),
            json!(draw)
        );
        let mut bound = 0;
        // The enum DRAW_FRAMEBUFFER_BINDING equals FRAMEBUFFER_BINDING, not DRAW.
        // Check native state through its binding pname as well as logical state.
        unsafe {
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut bound);
        }
        assert_eq!(
            bound as u32,
            context.objects.name(draw, Kind::Framebuffer).unwrap()
        );
        assert!(
            sample(&mut context, VOLUME, "vec3(0.5,0.5,0.5)", 0)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
    });
}

#[test]
fn webgl2_volume_copy_validates_read_routes_and_formats_even_at_zero_extent() {
    session::run_native_test(|| {
        let mut context = version_two();
        storage(&mut context, VOLUME);
        call(&mut context, "readBuffer", &[gl::NONE as i64], "");
        for source in [[0, 0, 4, 4], [0, 0, 0, 0]] {
            assert_eq!(copy(&mut context, source), Err(gl::INVALID_OPERATION));
        }
        call(&mut context, "readBuffer", &[gl::BACK as i64], "");
        assert_eq!(
            copy(&mut context, [i32::MAX as i64, 0, 1, 1]),
            Err(gl::INVALID_VALUE)
        );
        assert_eq!(
            copy(&mut context, [0, i32::MAX as i64, 1, 1]),
            Err(gl::INVALID_VALUE)
        );
        framebuffer(&mut context, 0x8d7c); // RGBA8UI cannot copy into normalized RGBA8.
        for source in [[0, 0, 4, 4], [0, 0, 0, 0]] {
            assert_eq!(copy(&mut context, source), Err(gl::INVALID_OPERATION));
        }
        assert!(
            sample(&mut context, VOLUME, "vec3(0.5,0.5,0.5)", 0)
                .chunks_exact(4)
                .all(|p| p == [0, 0, 0, 0])
        );
    });
}
