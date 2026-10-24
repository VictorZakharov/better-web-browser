//! Read enum rejection must leave both GPU buffer storage and CPU padding intact.
use super::api_version_tests::{call, version_two};
use super::*;

fn read(format: u32, kind: u32, pbo: bool) -> Command {
    Command {
        op: if pbo {
            "readPixelsToBuffer"
        } else {
            "readPixels"
        }
        .into(),
        i: vec![
            0,
            0,
            1,
            1,
            format as i64,
            kind as i64,
            if pbo { 0 } else { 4 },
        ],
        f: vec![],
        text: String::new(),
    }
}

#[test]
fn webgl2_pixel_read_enum_errors_are_shared_by_cpu_and_pixel_buffer_overloads() {
    session::run_native_test(|| {
        let mut context = version_two();
        let buffer = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        for pbo in [false, true] {
            call(
                &mut context,
                "bindBuffer",
                &[
                    core_buffers::PIXEL_PACK as i64,
                    if pbo { buffer } else { 0 },
                ],
                "",
            );
            if pbo {
                context
                    .dispatch(
                        &Command {
                            op: "bufferData".into(),
                            i: vec![core_buffers::PIXEL_PACK as i64, 4, gl::STATIC_DRAW as i64],
                            f: vec![],
                            text: String::new(),
                        },
                        Some(&[11, 22, 33, 44]),
                    )
                    .unwrap();
            }
            for (format, kind) in [
                (0x1902, gl::UNSIGNED_BYTE),
                (0x84f9, gl::UNSIGNED_BYTE),
                (0x8229, gl::UNSIGNED_BYTE),
                (gl::RGBA4, gl::UNSIGNED_BYTE),
                (gl::LUMINANCE, gl::UNSIGNED_BYTE),
                (gl::LUMINANCE_ALPHA, gl::UNSIGNED_BYTE),
                (gl::RGBA, 0x84fa),
                (gl::RGBA, 0x8dad),
                (0xdead, gl::UNSIGNED_BYTE),
                (gl::RGBA, 0xdead),
            ] {
                let command = read(format, kind, pbo);
                let result = if pbo {
                    context.dispatch(&command, None)
                } else {
                    context
                        .read_pixels(&command, Some(&[11, 22, 33, 44]))
                        .map(|_| Value::Null)
                };
                assert_eq!(
                    result,
                    Err(gl::INVALID_ENUM),
                    "format {format:x} type {kind:x}"
                );
            }
            // A known but unsupported pair is not an unknown enum. This remains
            // distinct from the extension-independent RGBA8 mandatory pair.
            let command = read(core_texture_formats::RGBA_INTEGER, gl::INT, pbo);
            let result = if pbo {
                context.dispatch(&command, None)
            } else {
                context
                    .read_pixels(&command, Some(&[11, 22, 33, 44]))
                    .map(|_| Value::Null)
            };
            assert_eq!(result, Err(gl::INVALID_OPERATION));
            if pbo {
                let bytes = context
                    .read_buffer(&Command {
                        op: "getBufferSubData".into(),
                        i: vec![core_buffers::PIXEL_PACK as i64, 0, 4],
                        f: vec![],
                        text: String::new(),
                    })
                    .unwrap();
                assert_eq!(bytes, [11, 22, 33, 44]);
            }
        }
        call(
            &mut context,
            "bindBuffer",
            &[core_buffers::PIXEL_PACK as i64, 0],
            "",
        );
        assert_eq!(
            context
                .read_pixels(&read(gl::RGBA, gl::UNSIGNED_BYTE, false), Some(&[99; 4]))
                .unwrap(),
            [0; 4]
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_complete_framebuffer_missing_selected_color_image_rejects_both_read_overloads() {
    session::run_native_test(|| {
        let mut context = version_two();
        super::typed_framebuffer_tests::image(&mut context, 0x8058);
        call(
            &mut context,
            "readBuffer",
            &[gl::COLOR_ATTACHMENT0 as i64 + 1],
            "",
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        assert_eq!(
            call(
                &mut context,
                "checkFramebufferStatus",
                &[gl::FRAMEBUFFER as i64],
                ""
            ),
            json!(gl::FRAMEBUFFER_COMPLETE)
        );
        let buffer = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        for pbo in [false, true] {
            call(
                &mut context,
                "bindBuffer",
                &[
                    core_buffers::PIXEL_PACK as i64,
                    if pbo { buffer } else { 0 },
                ],
                "",
            );
            if pbo {
                context
                    .dispatch(
                        &Command {
                            op: "bufferData".into(),
                            i: vec![core_buffers::PIXEL_PACK as i64, 4, gl::STATIC_DRAW as i64],
                            f: vec![],
                            text: String::new(),
                        },
                        Some(&[11, 22, 33, 44]),
                    )
                    .unwrap();
            }
            for selector in [gl::NONE, gl::COLOR_ATTACHMENT0 + 1] {
                call(&mut context, "readBuffer", &[selector as i64], "");
                let command = read(gl::RGBA, gl::UNSIGNED_BYTE, pbo);
                let result = if pbo {
                    context.dispatch(&command, None)
                } else {
                    context
                        .read_pixels(&command, Some(&[11, 22, 33, 44]))
                        .map(|_| Value::Null)
                };
                assert_eq!(result, Err(gl::INVALID_OPERATION));
                assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
            }
            if pbo {
                let actual = context
                    .read_buffer(&Command {
                        op: "getBufferSubData".into(),
                        i: vec![core_buffers::PIXEL_PACK as i64, 0, 4],
                        f: vec![],
                        text: String::new(),
                    })
                    .unwrap();
                assert_eq!(actual, [11, 22, 33, 44]);
            }
        }
        call(
            &mut context,
            "bindBuffer",
            &[core_buffers::PIXEL_PACK as i64, 0],
            "",
        );
        call(
            &mut context,
            "readBuffer",
            &[gl::COLOR_ATTACHMENT0 as i64],
            "",
        );
        assert_eq!(
            context
                .read_pixels(&read(gl::RGBA, gl::UNSIGNED_BYTE, false), Some(&[99; 4]))
                .unwrap(),
            [0; 4]
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
