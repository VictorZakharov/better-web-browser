use super::*;

#[test]
fn json_geometry_keeps_the_exact_binary_double_at_transformed_texel_boundaries() {
    let expected = [
        0.9696969696969697_f64,
        -0.12121212121212122,
        0.24242424242424243,
        0.9696969696969697,
        -7.030303030303031,
        -0.12121212121212122,
    ];
    let parsed: [f64; 6] = serde_json::from_str(
        "[0.9696969696969697,-0.12121212121212122,0.24242424242424243,0.9696969696969697,-7.030303030303031,-0.12121212121212122]").unwrap();
    assert_eq!(parsed.map(f64::to_bits), expected.map(f64::to_bits));
}

fn request() -> Request {
    Request {
        width: 4,
        height: 2,
        region: [0, 0, 4, 2],
        inverse: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        scale: 1.0,
        opacity: 1.0,
        operator: "source-over".into(),
        paint: [200.0, 30.0, 40.0, 255.0],
        opaque: false,
        stroke: false,
    }
}
fn packet(data: &[u8], color: bool) -> JsValue {
    JsValue::Array(vec![
        JsValue::from(1.0),
        JsValue::from(0.0),
        JsValue::from(2),
        JsValue::from(2),
        JsValue::Array(vec![
            JsValue::from(0),
            JsValue::from(0),
            JsValue::from(4),
            JsValue::from(2),
        ]),
        JsValue::from(color),
        JsValue::Bytes(data.to_vec()),
    ])
}

#[test]
fn masks_preserve_scalar_coverage_and_output_ownership() {
    let destination = [0; 32];
    let glyph = packet(&[255, 128, 0, 64], false);
    let mut output = render(&request(), &destination, &[glyph], None).unwrap();
    assert_eq!(&output[4..12], &[200, 30, 40, 255, 200, 30, 40, 128]);
    assert_eq!(&output[20..28], &[0, 0, 0, 0, 200, 30, 40, 64]);
    output.fill(0);
    assert_eq!(destination, [0; 32]);
}

#[test]
fn cropped_transfer_matches_full_render_without_changing_sampling_or_clip_stride() {
    let destination = [17; 32];
    let packet = packet(&[255, 128, 0, 64], false);
    let full = render(
        &request(),
        &destination,
        std::slice::from_ref(&packet),
        Some(&[0b0100_0010]),
    )
    .unwrap();
    let mut cropped = request();
    cropped.region = [1, 0, 3, 2];
    let region = [17; 16];
    let output = render(&cropped, &region, &[packet], Some(&[0b0100_0010])).unwrap();
    assert_eq!(&output[..8], &full[4..12]);
    assert_eq!(&output[8..], &full[20..28]);
    cropped.region = [3, 0, 2, 2];
    assert!(render(&cropped, &region, &[], None).is_none());
    cropped.region = [0, 0, 5, 2];
    assert!(render(&cropped, &region, &[], None).is_none());
}

#[test]
fn color_glyphs_keep_their_own_color_unless_stroked() {
    let glyph = packet(
        &[
            10, 20, 30, 255, 40, 50, 60, 128, 70, 80, 90, 0, 100, 110, 120, 64,
        ],
        true,
    );
    let output = render(&request(), &[0; 32], &[glyph], None).unwrap();
    assert_eq!(&output[4..12], &[10, 20, 30, 255, 40, 50, 60, 128]);
    let mut stroke = request();
    stroke.stroke = true;
    let output = render(
        &stroke,
        &[0; 32],
        &[packet(
            &[
                10, 20, 30, 255, 40, 50, 60, 128, 70, 80, 90, 0, 100, 110, 120, 64,
            ],
            true,
        )],
        None,
    )
    .unwrap();
    assert_eq!(&output[4..12], &[200, 30, 40, 255, 200, 30, 40, 255]);
}

#[test]
fn clips_and_transforms_use_full_bitmap_stride() {
    let output = render(
        &request(),
        &[17; 32],
        &[packet(&[255; 4], false)],
        Some(&[0b0100_0010]),
    )
    .unwrap();
    for pixel in 0..8 {
        let expected = if pixel == 1 || pixel == 6 {
            [200, 30, 40, 255]
        } else {
            [17; 4]
        };
        assert_eq!(&output[pixel * 4..pixel * 4 + 4], &expected);
    }
    let mut scaled = request();
    scaled.scale = 2.0;
    let output = render(
        &scaled,
        &[0; 32],
        &[packet(&[255, 128, 0, 64], false)],
        None,
    )
    .unwrap();
    assert_eq!(
        &output[4..16],
        &[200, 30, 40, 255, 200, 30, 40, 255, 200, 30, 40, 128]
    );
}

#[test]
fn malformed_late_glyph_is_rejected_before_any_result_is_returned() {
    let destination = [17; 32];
    for invalid in [
        JsValue::Null,
        packet(&[255; 3], false),
        packet(&[255; 4], true),
    ] {
        assert!(
            render(
                &request(),
                &destination,
                &[packet(&[255; 4], false), invalid],
                None
            )
            .is_none()
        );
        assert_eq!(destination, [17; 32]);
    }
    let mut invalid = request();
    invalid.inverse[0] = f64::NAN;
    assert!(render(&invalid, &destination, &[], None).is_none());
    invalid = request();
    invalid.paint[3] = 256.0;
    assert!(render(&invalid, &destination, &[], None).is_none());
    invalid = request();
    invalid.scale = 0.0;
    assert!(render(&invalid, &destination, &[], None).is_none());
    invalid = request();
    invalid.operator = "unknown".into();
    assert!(render(&invalid, &destination, &[], None).is_none());
    assert!(render(&request(), &destination, &[], Some(&[])).is_none());
    assert!(render(&request(), &destination[..31], &[], None).is_none());
}

#[test]
fn opaque_output_composites_premultiplied_color_without_exposing_alpha() {
    let mut opaque = request();
    opaque.opaque = true;
    opaque.opacity = 0.5;
    let destination = [10, 20, 30, 255].repeat(8);
    let output = render(&opaque, &destination, &[packet(&[255; 4], false)], None).unwrap();
    assert_eq!(&output[4..8], &[105, 25, 35, 255]);
    assert!(output.chunks_exact(4).all(|pixel| pixel[3] == 255));
    assert!(render(&opaque, &[0; 32], &[], None).is_none());
}
