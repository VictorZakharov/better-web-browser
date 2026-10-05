//! Readback reuse cannot bypass pack validation, route selection or resource caps.
use super::api_version_tests::{call, version_two};
use super::*;

fn pixels(context: &mut WebGl, x: i32, width: i32, height: i32, bytes: &[u8]) -> Result<Vec<u8>> {
    context.read_pixels(
        &Command {
            op: "readPixels".into(),
            i: vec![
                x.into(),
                0,
                width.into(),
                height.into(),
                gl::RGBA.into(),
                gl::UNSIGNED_BYTE.into(),
                bytes.len() as i64,
            ],
            f: vec![],
            text: String::new(),
        },
        Some(bytes),
    )
}

#[test]
fn cached_default_readback_preserves_pack_padding_and_rejects_missing_destination_bytes() {
    session::run_native_test(|| {
        let mut context = version_two();
        super::volume_copy_tests::clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        let red = pixels(&mut context, 0, 2, 2, &[0; 16]).unwrap();
        let charge = context.resource_bytes;
        assert_eq!(pixels(&mut context, 0, 2, 2, &[99; 16]).unwrap(), red);
        assert_eq!(context.resource_bytes, charge);
        assert_eq!(
            pixels(&mut context, 0, 2, 2, &[0; 15]),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "pixelStorei", &[0x0d02, 3], "");
        call(&mut context, "pixelStorei", &[0x0d04, 1], "");
        let padded = pixels(&mut context, 0, 2, 2, &[99; 24]).unwrap();
        assert_eq!(&padded[..4], &[99; 4]);
        assert_eq!(&padded[4..12], &red[..8]);
        assert_eq!(&padded[12..16], &[99; 4]);
        assert_eq!(&padded[16..], &red[8..]);
        call(&mut context, "pixelStorei", &[0x0d02, 0], "");
        call(&mut context, "pixelStorei", &[0x0d04, 0], "");
        assert_eq!(pixels(&mut context, 0, 2, 2, &[0; 16]).unwrap(), red);
    });
}

#[test]
fn cached_default_readback_never_hides_none_routes_or_pack_buffer_overload_errors() {
    session::run_native_test(|| {
        let mut context = version_two();
        super::volume_copy_tests::clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        let green = pixels(&mut context, 0, 1, 1, &[0; 4]).unwrap();
        call(&mut context, "readBuffer", &[gl::NONE.into()], "");
        assert_eq!(
            pixels(&mut context, 0, 1, 1, &[0; 4]),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "readBuffer", &[gl::BACK.into()], "");
        assert_eq!(pixels(&mut context, 0, 1, 1, &[0; 4]).unwrap(), green);
        let buffer = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        call(&mut context, "bindBuffer", &[0x88eb, buffer], "");
        call(
            &mut context,
            "bufferData",
            &[0x88eb, 4, gl::STATIC_DRAW.into()],
            "",
        );
        assert_eq!(
            pixels(&mut context, 0, 1, 1, &[0; 4]),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "bindBuffer", &[0x88eb, 0], "");
        assert_eq!(pixels(&mut context, 0, 1, 1, &[0; 4]).unwrap(), green);
    });
}

#[test]
fn readback_cache_budget_exhaustion_is_an_optimization_miss_not_a_read_error() {
    session::run_native_test(|| {
        let mut context = version_two();
        super::volume_copy_tests::clear(&mut context, [1.0, 0.0, 1.0, 1.0]);
        let original = context.resource_bytes;
        context.resource_limit = original + 7;
        for _ in 0..4 {
            assert_eq!(
                pixels(&mut context, 0, 1, 1, &[0; 4]).unwrap(),
                [255, 0, 255, 255]
            );
            assert_eq!(context.resource_bytes, original);
        }
        context.resource_limit = original + 8;
        assert_eq!(
            pixels(&mut context, 0, 1, 1, &[0; 4]).unwrap(),
            [255, 0, 255, 255]
        );
        assert_eq!(context.resource_bytes, original + 8);
        // A different region cannot return the old sample, and its successful
        // same-sized cache replacement does not increase charged capacity.
        call(&mut context, "enable", &[gl::SCISSOR_TEST.into()], "");
        call(&mut context, "scissor", &[1, 0, 1, 1], "");
        super::volume_copy_tests::clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(
            pixels(&mut context, 1, 1, 1, &[0; 4]).unwrap(),
            [0, 255, 0, 255]
        );
        assert_eq!(
            pixels(&mut context, 0, 1, 1, &[0; 4]).unwrap(),
            [255, 0, 255, 255]
        );
        assert_eq!(context.resource_bytes, original + 8);
    });
}
