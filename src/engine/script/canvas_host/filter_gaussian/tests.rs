use super::*;

fn request(width: u32, height: u32, sigma: f64) -> Request {
    Request {
        width,
        height,
        sigma,
    }
}

#[test]
fn gaussian_impulse_has_symmetric_nonuniform_falloff_and_preserves_straight_color() {
    let mut source = vec![0; 17 * 17 * 4];
    source[(8 * 17 + 8) * 4..(8 * 17 + 8) * 4 + 4].copy_from_slice(&[19, 71, 193, 255]);
    let result = render(&request(17, 17, 1.0), Cow::Borrowed(&source)).unwrap();
    let pixel = |x: usize, y: usize| &result[(y * 17 + x) * 4..(y * 17 + x) * 4 + 4];
    assert!(pixel(8, 8)[3] > pixel(9, 8)[3]);
    assert!(pixel(9, 8)[3] > pixel(10, 8)[3]);
    assert!(pixel(10, 8)[3] > 0);
    assert_eq!(pixel(7, 8), pixel(9, 8));
    assert_eq!(pixel(8, 7), pixel(8, 9));
    for value in result.chunks_exact(4).filter(|p| p[3] != 0) {
        assert_eq!(&value[..3], &[19, 71, 193]);
    }
    assert_eq!(pixel(0, 0), &[0; 4]);
}

#[test]
fn transparent_border_never_replicates_the_edge_or_leaks_hidden_rgb() {
    let opaque = render(&request(1, 1, 1.0), Cow::Borrowed(&[255, 0, 0, 255])).unwrap();
    assert!((30..=50).contains(&opaque[3]), "{opaque:?}");
    assert_eq!(&opaque[..3], &[255, 0, 0]);
    let source = [17, 251, 83, 0, 255, 0, 0, 255, 33, 77, 251, 0];
    let result = render(&request(3, 1, 1.0), Cow::Borrowed(&source)).unwrap();
    for pixel in result.chunks_exact(4) {
        assert_eq!(&pixel[..3], &[255, 0, 0]);
    }
    assert_eq!(source[0], 17);
}

#[test]
fn identity_preserves_storage_hidden_channels_and_tiny_sigma_without_library_defaults() {
    for sigma in [0.0, f64::MIN_POSITIVE, 0.0001] {
        let source = vec![17, 251, 83, 0, 31, 71, 113, 127];
        let pointer = source.as_ptr();
        let result = render(&request(2, 1, sigma), Cow::Owned(source.clone())).unwrap();
        assert_eq!(result, source);
        let result = render(&request(2, 1, sigma), Cow::Owned(source)).unwrap();
        assert_eq!(result.as_ptr(), pointer);
    }
}

#[test]
fn owned_and_borrowed_inputs_are_independent_and_only_owned_source_is_consumed() {
    let source = [31, 71, 113, 127].repeat(7 * 5);
    let mut args = vec![
        JsValue::from("canvasFilterGaussian".to_owned()),
        JsValue::Bytes(source.clone()),
        JsValue::from(r#"{"width":7,"height":5,"sigma":0.75}"#.to_owned()),
    ];
    let original = args.clone();
    let expected = paint(&args);
    assert_eq!(args, original);
    let pointer = args[1].as_bytes().unwrap().as_ptr();
    let actual = paint_owned(&mut args);
    assert_eq!(actual, expected);
    assert_eq!(actual.as_bytes().unwrap().as_ptr(), pointer);
    assert_eq!(args[1], JsValue::Null);
    assert_eq!(args[2], original[2]);
}

#[test]
fn dimensions_working_sets_parameters_and_wire_shapes_decline_atomically() {
    for invalid in [
        request(0, 1, 1.0),
        request(16385, 1, 1.0),
        request(u32::MAX, 1, 1.0),
        request(1, 1, -1.0),
        request(1, 1, f64::NAN),
        request(1, 1, f64::INFINITY),
        request(1, 1, 65.0),
    ] {
        assert!(render(&invalid, Cow::Borrowed(&[31, 71, 113, 127])).is_none());
    }
    assert!(render(&request(1, 1, 1.0), Cow::Borrowed(&[0; 3])).is_none());
    assert!(
        render(
            &request(1024, 1024, 32.0),
            Cow::Owned(vec![0; 1024 * 1024 * 4])
        )
        .is_none()
    );
    for encoded in [
        "{}",
        "[]",
        "null",
        r#"{"width":1,"height":1,"sigma":1,"extra":true}"#,
        r#"{"width":1.5,"height":1,"sigma":1}"#,
    ] {
        let mut args = vec![
            JsValue::Null,
            JsValue::Bytes(vec![31, 71, 113, 127]),
            JsValue::from(encoded.to_owned()),
        ];
        let original = args.clone();
        assert_eq!(paint(&args), JsValue::Null);
        assert_eq!(args, original);
        assert_eq!(paint_owned(&mut args), JsValue::Null);
        assert_eq!(args[2], original[2]);
    }
    for length in 0..3 {
        let mut args = vec![
            JsValue::Null,
            JsValue::Bytes(vec![0; 4]),
            JsValue::from("{}".to_owned()),
        ];
        args.truncate(length);
        assert_eq!(paint(&args), JsValue::Null);
        assert_eq!(paint_owned(&mut args), JsValue::Null);
    }
}
