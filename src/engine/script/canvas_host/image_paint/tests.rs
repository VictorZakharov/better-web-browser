use super::*;

fn request() -> Request {
    Request {
        width: 4,
        height: 2,
        source_width: 2,
        source_height: 1,
        bounds: [0, 0, 4, 2],
        inverse: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        source: [0.0, 0.0, 2.0, 1.0],
        destination: [0.0, 0.0, 4.0, 2.0],
        opacity: 1.0,
        smooth: false,
        operator: "source-over".into(),
    }
}

#[test]
fn nearest_scaled_image_owns_pixels_and_preserves_texel_boundaries() {
    let source = [255, 0, 0, 255, 0, 0, 255, 255];
    let destination = [0; 32];
    let mut result = render(&request(), &destination, &source, None).unwrap();
    for row in result.chunks_exact(16) {
        assert_eq!(
            row,
            [
                255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255
            ]
        );
    }
    result.fill(42);
    assert_eq!(destination, [0; 32]);
    assert_eq!(source, [255, 0, 0, 255, 0, 0, 255, 255]);
}

#[test]
fn smoothing_uses_premultiplied_color_and_does_not_bleed_hidden_rgb() {
    let source = [255, 0, 0, 255, 0, 255, 0, 0];
    let mut request = request();
    request.smooth = true;
    let result = render(&request, &[0; 32], &source, None).unwrap();
    assert_eq!(
        &result[..16],
        &[255, 0, 0, 255, 255, 0, 0, 191, 255, 0, 0, 64, 0, 0, 0, 0]
    );
    assert_eq!(
        sample::pixel(&[9, 20, 30, 0], 1, 1, -0.25, -0.25, true),
        [0.0; 4]
    );
}

#[test]
fn clipping_uses_full_target_stride_and_keeps_outside_bytes() {
    let source = [255, 0, 0, 255, 0, 0, 255, 255];
    let backdrop = [17; 32];
    let result = render(&request(), &backdrop, &source, Some(&[0b1000_0010])).unwrap();
    for index in 0..8 {
        let actual = &result[index * 4..index * 4 + 4];
        match index {
            1 => assert_eq!(actual, [255, 0, 0, 255]),
            7 => assert_eq!(actual, [0, 0, 255, 255]),
            _ => assert_eq!(actual, [17; 4]),
        }
    }
}

#[test]
fn bounds_transform_and_partial_source_crop_preserve_unpainted_backdrop() {
    let mut request = request();
    request.bounds = [1, 0, 3, 2];
    request.inverse[4] = -1.0;
    request.source[0] = -1.0;
    let result = render(&request, &[17; 32], &[255; 8], None).unwrap();
    assert_eq!(&result[..12], &[17; 12]);
    assert_eq!(&result[12..16], &[17; 4]);
    // Only the last included column maps to an in-image source center.
    assert_eq!(&result[24..28], &[17; 4]);
    request.source[0] = 0.0;
    let result = render(&request, &[17; 32], &[255; 8], None).unwrap();
    assert_eq!(&result[4..12], &[255; 8]);
    assert_eq!(&result[..4], &[17; 4]);
    assert_eq!(&result[12..16], &[17; 4]);
}

#[test]
fn opacity_and_operator_share_the_existing_pixel_contract() {
    let mut request = request();
    request.opacity = 0.5;
    request.operator = "destination-over".into();
    let destination = [10, 20, 30, 128].repeat(8);
    let source = [200, 80, 40, 128].repeat(2);
    let result = render(&request, &destination, &source, None).unwrap();
    let mut expected = [10, 20, 30, 128];
    Operator::DestinationOver.pixel(&mut expected, [200.0, 80.0, 40.0, 128.0], 0.5);
    assert!(result.chunks_exact(4).all(|pixel| pixel == expected));
}

#[test]
fn invalid_shapes_numbers_clips_and_operators_fail_closed() {
    let mut request = request();
    assert!(render(&request, &[0; 31], &[0; 8], None).is_none());
    assert!(render(&request, &[0; 32], &[0; 7], None).is_none());
    assert!(render(&request, &[0; 32], &[0; 8], Some(&[])).is_none());
    request.bounds[2] = 5;
    assert!(render(&request, &[0; 32], &[0; 8], None).is_none());
    request.bounds[2] = 4;
    request.inverse[0] = f64::NAN;
    assert!(render(&request, &[0; 32], &[0; 8], None).is_none());
    request.inverse[0] = 1.0;
    request.opacity = 1.01;
    assert!(render(&request, &[0; 32], &[0; 8], None).is_none());
    request.opacity = 1.0;
    request.destination[2] = 0.0;
    assert!(render(&request, &[0; 32], &[0; 8], None).is_none());
    request.destination[2] = 4.0;
    request.operator = "not-a-mode".into();
    assert!(render(&request, &[0; 32], &[0; 8], None).is_none());
    assert!(bitmap_pixels(0, 1, &[]).is_none());
    assert!(bitmap_pixels(u32::MAX, u32::MAX, &[]).is_none());
}

#[test]
fn bridge_rejects_unexpected_metadata_and_missing_storage() {
    assert_eq!(paint(&[]), JsValue::Null);
    let args = [
        JsValue::from("canvasPaintImage".to_string()),
        JsValue::from("{}".to_string()),
        JsValue::Bytes(vec![0; 32]),
        JsValue::Bytes(vec![0; 8]),
        JsValue::Null,
    ];
    assert_eq!(paint(&args), JsValue::Null);
    let oversized = [JsValue::Null, JsValue::from(" ".repeat(2049))];
    assert_eq!(paint(&oversized), JsValue::Null);
}

#[test]
fn sampler_clamps_edges_and_rounds_half_way_nearest_like_javascript() {
    let source = [10, 20, 30, 255, 40, 50, 60, 255];
    assert_eq!(
        sample::pixel(&source, 2, 1, -0.5, 0.0, false),
        [10.0, 20.0, 30.0, 255.0]
    );
    assert_eq!(
        sample::pixel(&source, 2, 1, 0.5, 0.0, false),
        [40.0, 50.0, 60.0, 255.0]
    );
    assert_eq!(
        sample::pixel(&source, 2, 1, -0.25, 0.0, true),
        [10.0, 20.0, 30.0, 255.0]
    );
    assert_eq!(
        sample::pixel(&source, 2, 1, 0.5, 0.0, true),
        [25.0, 35.0, 45.0, 255.0]
    );
}
