use crate::engine::webgl::api_version_tests::{call, version_two};
use crate::engine::webgl::*;
use std::borrow::Cow;

fn command(width: i64, height: i64, capacity: i64) -> Command {
    Command {
        op: "readPixels".into(),
        i: vec![
            0,
            0,
            width,
            height,
            gl::RGBA as i64,
            gl::UNSIGNED_BYTE as i64,
            capacity,
        ],
        f: vec![],
        text: String::new(),
    }
}

#[test]
fn owned_pixel_readback_moves_the_real_native_destination_allocation() {
    session::run_native_test(|| {
        let mut context = version_two();
        context
            .dispatch(
                &Command {
                    op: "clearColor".into(),
                    i: vec![],
                    f: vec![1., 0., 0., 1.],
                    text: String::new(),
                },
                None,
            )
            .unwrap();
        call(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
        let bytes = vec![83; 64];
        let pointer = bytes.as_ptr();
        let result = context
            .read_pixels_data(&command(4, 4, 64), Some(Cow::Owned(bytes)))
            .unwrap();
        assert_eq!(result.as_ptr(), pointer);
        assert!(
            result
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn borrowed_pixel_readback_never_changes_the_callers_bytes() {
    session::run_native_test(|| {
        let mut context = version_two();
        let bytes = vec![83; 64];
        let result = context
            .read_pixels(&command(4, 4, 64), Some(&bytes))
            .unwrap();
        assert_ne!(result.as_ptr(), bytes.as_ptr());
        assert!(result.iter().all(|byte| *byte == 0));
        assert!(bytes.iter().all(|byte| *byte == 83));
    });
}

#[test]
fn owned_pixel_readback_returns_only_the_admitted_layout_and_exact_capacity() {
    session::run_native_test(|| {
        let mut context = version_two();
        let mut bytes = Vec::with_capacity(1024 * 1024);
        bytes.extend_from_slice(&[83; 128]);
        let result = context
            .read_pixels_data(&command(4, 4, 128), Some(Cow::Owned(bytes)))
            .unwrap();
        assert_eq!(result.len(), 64);
        assert_eq!(result.capacity(), 64);
        assert!(result.iter().all(|byte| *byte == 0));
    });
}

#[test]
fn owned_pixel_readback_validates_size_before_any_native_write() {
    session::run_native_test(|| {
        let mut context = version_two();
        assert!(matches!(
            context.read_pixels_data(&command(4, 4, 64), Some(Cow::Owned(vec![83; 32]))),
            Err(gl::INVALID_OPERATION)
        ));
        assert!(matches!(
            context.read_pixels_data(&command(-1, 4, 64), Some(Cow::Owned(vec![83; 64]))),
            Err(gl::INVALID_VALUE)
        ));
        assert!(matches!(
            context.read_pixels_data(&command(4096, 4096, 64), None),
            Err(gl::OUT_OF_MEMORY)
        ));
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
