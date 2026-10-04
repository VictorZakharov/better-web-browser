//! Native GLES renderability must not silently broaden the WebGL2 color table.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{texture, upload};
use super::*;

fn attach(context: &mut WebGl, format: u32) -> u32 {
    let image = texture(context, gl::TEXTURE_2D);
    call(
        context,
        "texStorage2D",
        &[gl::TEXTURE_2D as i64, 1, format as i64, 4, 4],
        "",
    );
    let framebuffer = call(context, "createFramebuffer", &[], "")
        .as_u64()
        .unwrap() as u32;
    call(
        context,
        "bindFramebuffer",
        &[gl::FRAMEBUFFER as i64, framebuffer as i64],
        "",
    );
    call(
        context,
        "framebufferTexture2D",
        &[
            gl::FRAMEBUFFER as i64,
            gl::COLOR_ATTACHMENT0 as i64,
            gl::TEXTURE_2D as i64,
            image as i64,
            0,
        ],
        "",
    );
    framebuffer
}

#[test]
fn webgl2_rgb_float_storage_is_not_a_color_target_after_either_float_extension() {
    session::run_native_test(|| {
        let mut context = version_two();
        for extension in ["EXT_color_buffer_float", "EXT_color_buffer_half_float"] {
            assert_eq!(
                call(&mut context, "enableExtension", &[], extension),
                json!(true)
            );
            for format in [0x881b, 0x8815] {
                attach(&mut context, format);
                assert_ne!(
                    call(
                        &mut context,
                        "checkFramebufferStatus",
                        &[gl::FRAMEBUFFER as i64],
                        ""
                    ),
                    json!(gl::FRAMEBUFFER_COMPLETE),
                    "{extension}: {format:#x}"
                );
                assert_eq!(
                    upload(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], None),
                    Err(gl::INVALID_FRAMEBUFFER_OPERATION)
                );
            }
        }
    });
}

#[test]
fn webgl2_core_color_targets_do_not_require_a_legacy_extension() {
    session::run_native_test(|| {
        let mut context = version_two();
        for format in [
            0x8229, 0x822b, 0x8051, 0x8058, 0x8c43, 0x8d62, 0x8056, 0x8057, 0x8059, 0x8231, 0x8232,
            0x8233, 0x8234, 0x8235, 0x8236, 0x8237, 0x8238, 0x8239, 0x823a, 0x823b, 0x823c, 0x8d8e,
            0x8d7c, 0x8d88, 0x8d76, 0x8d82, 0x8d70, 0x906f,
        ] {
            attach(&mut context, format);
            assert_eq!(
                call(
                    &mut context,
                    "checkFramebufferStatus",
                    &[gl::FRAMEBUFFER as i64],
                    ""
                ),
                json!(gl::FRAMEBUFFER_COMPLETE),
                "{format:#x}"
            );
        }
    });
}

#[test]
fn webgl2_sample_only_formats_remain_nonrenderable_even_with_float_extensions() {
    session::run_native_test(|| {
        let mut context = version_two();
        call(
            &mut context,
            "enableExtension",
            &[],
            "EXT_color_buffer_float",
        );
        call(
            &mut context,
            "enableExtension",
            &[],
            "EXT_color_buffer_half_float",
        );
        for format in [
            0x8f94, 0x8f95, 0x8f96, 0x8f97, 0x8c41, 0x8c3d, 0x8d8f, 0x8d7d, 0x8d89, 0x8d77, 0x8d83,
            0x8d71,
        ] {
            attach(&mut context, format);
            assert_ne!(
                call(
                    &mut context,
                    "checkFramebufferStatus",
                    &[gl::FRAMEBUFFER as i64],
                    ""
                ),
                json!(gl::FRAMEBUFFER_COMPLETE),
                "{format:#x}"
            );
        }
    });
}

#[test]
fn webgl2_color_float_admission_changes_only_the_specified_hdr_formats() {
    session::run_native_test(|| {
        let mut context = version_two();
        let formats = [0x822d, 0x822f, 0x881a, 0x822e, 0x8230, 0x8814, 0x8c3a];
        for format in formats {
            attach(&mut context, format);
            assert_ne!(
                call(
                    &mut context,
                    "checkFramebufferStatus",
                    &[gl::FRAMEBUFFER as i64],
                    ""
                ),
                json!(gl::FRAMEBUFFER_COMPLETE),
                "disabled {format:#x}"
            );
        }
        assert_eq!(
            call(
                &mut context,
                "enableExtension",
                &[],
                "EXT_color_buffer_float"
            ),
            json!(true)
        );
        for format in formats {
            attach(&mut context, format);
            assert_eq!(
                call(
                    &mut context,
                    "checkFramebufferStatus",
                    &[gl::FRAMEBUFFER as i64],
                    ""
                ),
                json!(gl::FRAMEBUFFER_COMPLETE),
                "enabled {format:#x}"
            );
        }
    });
}
