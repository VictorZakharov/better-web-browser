use super::*;

fn request(sw: u32, sh: u32, width: u32, height: u32, smooth: bool) -> Request {
    Request {
        width,
        height,
        source_width: sw,
        source_height: sh,
        smooth,
    }
}

#[test]
fn nearest_resize_repeats_exact_source_pixels_including_transparent_rgb() {
    let source = [17, 31, 47, 255, 211, 197, 173, 0];
    assert_eq!(
        render(&request(2, 1, 4, 1, false), &source).unwrap(),
        [
            17, 31, 47, 255, 17, 31, 47, 255, 211, 197, 173, 0, 211, 197, 173, 0
        ]
    );
    assert_eq!(
        render(&request(2, 1, 1, 1, false), &source).unwrap(),
        source[4..]
    );
}

#[test]
fn smooth_resize_uses_associated_alpha_and_canonicalizes_zero_alpha() {
    let source = [255, 0, 0, 255, 0, 0, 255, 0];
    assert_eq!(
        render(&request(2, 1, 3, 1, true), &source).unwrap(),
        [255, 0, 0, 255, 255, 0, 0, 128, 0, 0, 0, 0]
    );
    let invisible = [255, 127, 63, 0];
    assert_eq!(
        render(&request(1, 1, 3, 2, true), &invisible).unwrap(),
        vec![0; 24]
    );
}

#[test]
fn smooth_quantization_preserves_uint8_clamped_half_way_rounding() {
    let source = [0, 1, 2, 255, 1, 2, 3, 255];
    assert_eq!(
        render(&request(2, 1, 1, 1, true), &source).unwrap(),
        [0, 2, 2, 255]
    );
}

// Independent scalar transcription of the previous JS contract. Do not call
// the shared native sampler: this oracle must catch changes in its tap order,
// edge convention, alpha weighting, or half-way nearest selection.
fn scalar(source: &[u8], r: &Request) -> Vec<u8> {
    let mut output = Vec::new();
    for row in 0..r.height {
        for column in 0..r.width {
            let x =
                (f64::from(column) + 0.5) * f64::from(r.source_width) / f64::from(r.width) - 0.5;
            let y = (f64::from(row) + 0.5) * f64::from(r.source_height) / f64::from(r.height) - 0.5;
            let read = |x: f64, y: f64| {
                let column = x.max(0.0).min(f64::from(r.source_width - 1)) as usize;
                let row = y.max(0.0).min(f64::from(r.source_height - 1)) as usize;
                let offset = (row * r.source_width as usize + column) * 4;
                std::array::from_fn::<_, 4, _>(|channel| f64::from(source[offset + channel]))
            };
            let value = if r.smooth {
                let (left, top) = (x.floor(), y.floor());
                let (fx, fy) = (x - left, y - top);
                let taps = [
                    (left, top, (1.0 - fx) * (1.0 - fy)),
                    (left + 1.0, top, fx * (1.0 - fy)),
                    (left, top + 1.0, (1.0 - fx) * fy),
                    (left + 1.0, top + 1.0, fx * fy),
                ];
                let mut alpha = 0.0;
                let mut color = [0.0; 3];
                for (x, y, weight) in taps {
                    let pixel = read(x, y);
                    let associated = pixel[3] * weight;
                    alpha += associated;
                    for channel in 0..3 {
                        color[channel] += pixel[channel] * associated;
                    }
                }
                if alpha == 0.0 {
                    [0.0; 4]
                } else {
                    [color[0] / alpha, color[1] / alpha, color[2] / alpha, alpha]
                }
            } else {
                read((x + 0.5).floor(), (y + 0.5).floor())
            };
            output.extend(value.map(|channel| channel.clamp(0.0, 255.0).round_ties_even() as u8));
        }
    }
    output
}

#[test]
fn bounded_dimension_grid_matches_the_previous_scalar_readback_contract() {
    for sw in 1..=5 {
        for sh in 1..=5 {
            let source: Vec<u8> = (0..sw * sh * 4)
                .map(|i| ((i * 79 + 31) % 256) as u8)
                .collect();
            for width in 1..=7 {
                for height in 1..=7 {
                    for smooth in [false, true] {
                        let r = request(sw, sh, width, height, smooth);
                        assert_eq!(
                            render(&r, &source).unwrap(),
                            scalar(&source, &r),
                            "{sw}x{sh} -> {width}x{height}, smooth={smooth}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn malformed_wire_and_shapes_fail_before_sampling_without_changing_source() {
    let source = vec![17, 31, 47, 255];
    for encoded in [
        r#"{"width":1,"height":1,"source_width":1,"source_height":1,"smooth":true,"extra":0}"#,
        r#"{"width":1.5,"height":1,"source_width":1,"source_height":1,"smooth":true}"#,
        r#"{"width":1,"height":1,"source_width":1,"source_height":1}"#,
        "null",
        "{}",
    ] {
        let args = [
            JsValue::Null,
            JsValue::from(encoded.to_owned()),
            JsValue::Bytes(source.clone()),
        ];
        assert!(matches!(resize(&args), JsValue::Null));
        assert_eq!(args[2].as_bytes().unwrap(), source);
    }
    for r in [
        request(0, 1, 1, 1, true),
        request(1, 1, 0, 1, true),
        request(1, 1, MAX_CANVAS_PIXELS as u32 + 1, 1, true),
        request(1, 1, 4096, 4096, true),
        request(u32::MAX, u32::MAX, 1, 1, true),
        request(2, 1, 1, 1, true),
    ] {
        assert!(render(&r, &source).is_none());
    }
    assert!(render(&request(1, 1, 1, 1, true), &[17, 31, 47]).is_none());
    assert!(render(&request(1, 1, 1, 1, true), &[17, 31, 47, 255, 0]).is_none());
}

#[test]
fn thin_images_keep_the_existing_pixel_budget_without_an_extra_axis_limit() {
    let source = [17, 31, 47, 255];
    let output = render(&request(1, 1, 16385, 1, true), &source).unwrap();
    assert_eq!(output.len(), 16385 * 4);
    assert!(output.chunks_exact(4).all(|pixel| pixel == source));
}

#[test]
fn owned_and_borrowed_dispatch_match_without_retaining_or_mutating_author_storage() {
    let encoded = r#"{"width":3,"height":2,"source_width":2,"source_height":1,"smooth":true}"#;
    let source = vec![17, 31, 47, 255, 197, 113, 29, 128];
    let mut args = vec![
        JsValue::from("canvasResizeImage".to_owned()),
        JsValue::from(encoded.to_owned()),
        JsValue::Bytes(source.clone()),
    ];
    let borrowed = resize(&args);
    assert_eq!(args[2].as_bytes().unwrap(), source);
    let owned = resize_owned(&mut args);
    assert_eq!(borrowed.as_bytes(), owned.as_bytes());
    assert!(matches!(args[2], JsValue::Null));
}
