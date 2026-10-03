//! Padding/skip bytes and untouched slices are part of the pixel-transfer contract.
use super::api_version_tests::{call, version_two};
use super::pixel_layout::{Direction, Store};
use super::*;

#[test]
fn pixel_footprint_includes_alignment_skips_and_last_row_without_trailing_padding() {
    let state = Store {
        alignment: 4,
        row_length: 5,
        skip_pixels: 1,
        skip_rows: 2,
        image_height: 4,
        skip_images: 1,
    };
    let two = state.layout(2, 2, 1, 3, false).unwrap();
    assert_eq!((two.row_stride, two.start, two.size), (16, 35, 57));
    let three = state.layout(2, 2, 2, 3, true).unwrap();
    assert_eq!((three.row_stride, three.start, three.size), (16, 99, 185));
    let empty = state.layout(0, 2, 2, 3, true).unwrap();
    assert_eq!((empty.start, empty.size), (0, 0));
}

#[test]
fn pixel_footprint_rejects_overlapping_rows_images_and_arithmetic_overflow() {
    for state in [
        Store {
            alignment: 4,
            row_length: 1,
            ..Store::default()
        },
        Store {
            alignment: 4,
            skip_pixels: 1,
            ..Store::default()
        },
        Store {
            alignment: 4,
            image_height: 1,
            ..Store::default()
        },
        Store {
            alignment: 4,
            skip_rows: 1,
            ..Store::default()
        },
        Store {
            alignment: 4,
            row_length: usize::MAX,
            ..Store::default()
        },
        Store {
            alignment: 4,
            row_length: 4,
            skip_images: usize::MAX,
            ..Store::default()
        },
        Store {
            alignment: 3,
            ..Store::default()
        },
    ] {
        assert!(state.layout(2, 2, 2, 4, true).is_err());
    }
}

fn read(context: &mut WebGl, buffer: &[u8]) -> Result<Vec<u8>> {
    context.read_pixels(
        &Command {
            op: "readPixels".into(),
            i: vec![
                0,
                0,
                2,
                2,
                gl::RGBA as i64,
                gl::UNSIGNED_BYTE as i64,
                buffer.len() as i64,
            ],
            f: vec![],
            text: String::new(),
        },
        Some(buffer),
    )
}
#[test]
fn webgl2_packed_subrectangle_preserves_skipped_destination_bytes_and_private_capture() {
    session::run_native_test(|| {
        let mut context = version_two();
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
        for (pname, value) in [(0x0d02, 4), (0x0d03, 1), (0x0d04, 1)] {
            call(&mut context, "pixelStorei", &[pname, value], "");
        }
        let actual = read(&mut context, &[0xa5; 44]).unwrap();
        let mut expected = vec![0xa5; 44];
        for start in [20, 36] {
            for pixel in expected[start..start + 8].chunks_exact_mut(4) {
                pixel.copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        assert_eq!(actual, expected);
        assert_eq!(read(&mut context, &[0xa5; 43]), Err(gl::INVALID_OPERATION));
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [0, 0, 255, 255])
        );
        let state = Store::native(ApiVersion::Two, Direction::Pack).unwrap();
        assert_eq!(
            (state.row_length, state.skip_rows, state.skip_pixels),
            (4, 1, 1)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_owned_unpack_subrectangle_selects_real_pixels_and_short_input_is_atomic() {
    session::run_native_test(|| {
        let mut context = version_two();
        let texture = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindTexture",
            &[gl::TEXTURE_2D as i64, texture as i64],
            "",
        );
        for (pname, value) in [(0x0cf2, 4), (0x0cf3, 1), (0x0cf4, 1)] {
            call(&mut context, "pixelStorei", &[pname, value], "");
        }
        let command = Command {
            op: "texImage2D".into(),
            i: vec![
                gl::TEXTURE_2D as i64,
                0,
                0x8058,
                2,
                2,
                0,
                gl::RGBA as i64,
                gl::UNSIGNED_BYTE as i64,
            ],
            f: vec![],
            text: String::new(),
        };
        let mut bytes = vec![0xa5; 44];
        for start in [20, 36] {
            for pixel in bytes[start..start + 8].chunks_exact_mut(4) {
                pixel.copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        context.dispatch(&command, Some(&bytes)).unwrap();
        assert_eq!(
            context.dispatch(&command, Some(&bytes[..43])),
            Err(gl::INVALID_OPERATION)
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
        assert!(
            read(&mut context, &[0; 16])
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        let state = Store::native(ApiVersion::Two, Direction::Unpack).unwrap();
        assert_eq!(
            (state.row_length, state.skip_rows, state.skip_pixels),
            (4, 1, 1)
        );
    });
}

#[test]
fn webgl2_null_volume_allocation_ignores_skip_constraints_and_restores_private_zeroing_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        let texture = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap();
        call(&mut context, "bindTexture", &[0x8c1a, texture as i64], "");
        for (pname, value) in [
            (0x0cf2, 1),
            (0x0cf3, 9),
            (0x0cf4, 9),
            (0x806d, 9),
            (0x806e, 1),
        ] {
            call(&mut context, "pixelStorei", &[pname, value], "");
        }
        call(
            &mut context,
            "texImage3D",
            &[
                0x8c1a,
                0,
                0x8058,
                2,
                2,
                2,
                0,
                gl::RGBA as i64,
                gl::UNSIGNED_BYTE as i64,
            ],
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
        for layer in [0, 1] {
            call(
                &mut context,
                "framebufferTextureLayer",
                &[
                    gl::FRAMEBUFFER as i64,
                    gl::COLOR_ATTACHMENT0 as i64,
                    texture as i64,
                    0,
                    layer,
                ],
                "",
            );
            assert!(
                read(&mut context, &[0; 16])
                    .unwrap()
                    .iter()
                    .all(|byte| *byte == 0)
            );
        }
        let state = Store::native(ApiVersion::Two, Direction::Unpack).unwrap();
        assert_eq!(
            (
                state.row_length,
                state.skip_rows,
                state.skip_pixels,
                state.skip_images,
                state.image_height
            ),
            (1, 9, 9, 9, 1)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
