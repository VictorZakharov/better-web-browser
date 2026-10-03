//! Exact GPU replies prove signed, unsigned and floating clear semantics.
use super::api_version_tests::{call, version_two};
use super::*;

fn image(context: &mut WebGl, format: u32) {
    let texture = call(context, "createTexture", &[], "").as_u64().unwrap();
    call(
        context,
        "bindTexture",
        &[gl::TEXTURE_2D as i64, texture as i64],
        "",
    );
    call(
        context,
        "texStorage2D",
        &[gl::TEXTURE_2D as i64, 1, format as i64, 4, 4],
        "",
    );
    let framebuffer = call(context, "createFramebuffer", &[], "")
        .as_u64()
        .unwrap();
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
}
fn read(context: &mut WebGl, format: u32, kind: u32, size: i64) -> Result<Vec<u8>> {
    context.read_pixels(
        &Command {
            op: "readPixels".into(),
            i: vec![0, 0, 4, 4, format as i64, kind as i64, size],
            f: vec![],
            text: String::new(),
        },
        None,
    )
}
fn clear(context: &mut WebGl, op: &str, integers: &[i64], floats: &[f64]) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: integers.into(),
            f: floats.into(),
            text: String::new(),
        },
        None,
    )
}

#[test]
fn webgl2_unsigned_integer_clear_and_readback_preserve_all_32_bits() {
    session::run_native_test(|| {
        let mut context = version_two();
        image(&mut context, 0x8d70); // RGBA32UI
        clear(
            &mut context,
            "clearBufferuiv",
            &[0x1800, 0, u32::MAX as i64, 0x80000000, 7, 42],
            &[],
        )
        .unwrap();
        let bytes = read(&mut context, 0x8d99, gl::UNSIGNED_INT, 256).unwrap();
        let expected = [u32::MAX, 0x80000000, 7, 42].map(u32::to_ne_bytes).concat();
        assert!(bytes.chunks_exact(16).all(|pixel| pixel == expected));
        assert_eq!(
            read(&mut context, gl::RGBA, gl::UNSIGNED_BYTE, 64),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            read(&mut context, 0x8d99, gl::INT, 256),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            read(&mut context, 0x8d99, gl::UNSIGNED_INT, 255),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_signed_integer_clear_does_not_clamp_negative_values() {
    session::run_native_test(|| {
        let mut context = version_two();
        image(&mut context, 0x8d82); // RGBA32I
        clear(
            &mut context,
            "clearBufferiv",
            &[0x1800, 0, i32::MIN as i64, -7, 9, i32::MAX as i64],
            &[],
        )
        .unwrap();
        let bytes = read(&mut context, 0x8d99, gl::INT, 256).unwrap();
        let expected = [i32::MIN, -7, 9, i32::MAX].map(i32::to_ne_bytes).concat();
        assert!(bytes.chunks_exact(16).all(|pixel| pixel == expected));
        assert_eq!(
            read(&mut context, 0x8d99, gl::UNSIGNED_INT, 256),
            Err(gl::INVALID_OPERATION)
        );
    });
}

#[test]
fn webgl2_float_color_admission_preserves_hdr_half_and_full_float_targets() {
    session::run_native_test(|| {
        let mut context = version_two();
        assert_eq!(
            call(
                &mut context,
                "enableExtension",
                &[],
                "EXT_color_buffer_float"
            ),
            json!(true)
        );
        for format in [0x881a, 0x8814] {
            image(&mut context, format);
            clear(
                &mut context,
                "clearBufferfv",
                &[0x1800, 0],
                &[2., -1., 0.5, 4.],
            )
            .unwrap();
            let bytes = read(&mut context, gl::RGBA, gl::FLOAT, 256).unwrap();
            let expected = [2f32, -1., 0.5, 4.].map(f32::to_ne_bytes).concat();
            assert!(bytes.chunks_exact(16).all(|pixel| pixel == expected));
        }
        for format in [0x822d, 0x822e, 0x822f, 0x8230, 0x881a, 0x8814, 0x8c3a] {
            let buffer = call(&mut context, "createRenderbuffer", &[], "")
                .as_u64()
                .unwrap();
            call(
                &mut context,
                "bindRenderbuffer",
                &[gl::RENDERBUFFER as i64, buffer as i64],
                "",
            );
            call(
                &mut context,
                "renderbufferStorage",
                &[gl::RENDERBUFFER as i64, format, 4, 4],
                "",
            );
        }
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_typed_clear_validates_shape_and_indices_before_reading_arrays() {
    session::run_native_test(|| {
        let mut context = version_two();
        for (op, args, floats, error) in [
            (
                "clearBufferfv",
                vec![0x1800, 0],
                vec![1.],
                gl::INVALID_VALUE,
            ),
            (
                "clearBufferiv",
                vec![0x1800, 0, 1, 2, 3],
                vec![],
                gl::INVALID_VALUE,
            ),
            (
                "clearBufferuiv",
                vec![0x1800, -1, 1, 2, 3, 4],
                vec![],
                gl::INVALID_VALUE,
            ),
            (
                "clearBufferfv",
                vec![0x1801, 1],
                vec![1.],
                gl::INVALID_VALUE,
            ),
            (
                "clearBufferuiv",
                vec![0x1801, 0, 1, 2, 3, 4],
                vec![],
                gl::INVALID_ENUM,
            ),
        ] {
            assert_eq!(clear(&mut context, op, &args, &floats), Err(error));
        }
        let mut one = WebGl::new(4, 4, Options::default()).unwrap();
        assert_eq!(
            clear(&mut one, "clearBufferfv", &[0x1800, 0], &[1.; 4]),
            Err(gl::INVALID_OPERATION)
        );
    });
}
