//! Rectangle paint consumes no author geometry, bitmap, or path state.
use super::*;

fn request() -> Request {
    Request {
        width: 32,
        height: 32,
        left: 0,
        top: 0,
        rect: [0.25, 0.5, 30.5, 30.25],
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        color: Some([31.5, 205.1, 73.25, 183.0]),
        opacity: 0.37,
        opaque: false,
    }
}

#[test]
fn aligned_fractional_paint_matches_unquantized_scalar_area_on_every_pixel() {
    for rect in [
        [0.25, 0.5, 30.5, 30.25],
        [-0.375, 1.25, 20.5, 19.125],
        [0.0, 0.0, 32.0, 32.0],
    ] {
        for opacity in [0.0, 0.37, 1.0] {
            for source_alpha in [0.0, 0.5, 128.0, 255.0] {
                for backdrop_alpha in [0, 1, 127, 255] {
                    let mut request = request();
                    request.rect = rect;
                    request.opacity = opacity;
                    request.color.as_mut().unwrap()[3] = source_alpha;
                    let destination = [29, 63, 193, backdrop_alpha].repeat(1024);
                    let mut expected = destination.clone();
                    let [left, top, width, height] = rect;
                    for (index, pixel) in expected.chunks_exact_mut(4).enumerate() {
                        let x = (index % 32) as f64;
                        let y = (index / 32) as f64;
                        let coverage = ((x + 1.0).min(left + width) - x.max(left)).max(0.0)
                            * ((y + 1.0).min(top + height) - y.max(top)).max(0.0);
                        if coverage > 0.0 {
                            super::super::solid_mask::source_over(
                                pixel,
                                request.color.unwrap(),
                                opacity * coverage,
                            );
                        }
                    }
                    assert_eq!(
                        render(&request, Cow::Borrowed(&destination), None).unwrap(),
                        expected
                    );
                }
            }
        }
    }
}

#[test]
fn clear_ignores_source_alpha_and_preserves_opaque_output_mode() {
    for opaque in [false, true] {
        let mut request = request();
        request.color = None;
        request.opacity = 0.0;
        request.rect = [0.5, 0.0, 1.0, 1.0];
        request.opaque = opaque;
        let destination = [29, 63, 193, 255].repeat(1024);
        let actual = render(&request, Cow::Borrowed(&destination), None).unwrap();
        assert_eq!(&actual[..4], &destination[..4]);
        assert_eq!(&actual[4..8], &[0, 0, 0, if opaque { 255 } else { 0 }]);
        assert_eq!(&actual[8..], &destination[8..]);
    }
}

#[test]
fn affine_clear_uses_original_rectangle_pixel_centers_not_the_bounding_box() {
    let mut request = request();
    request.color = None;
    request.rect = [0.0, 0.0, 1.0, 1.0];
    request.matrix = [1.0, 0.0, 1.0, 1.0, 0.0, 0.0];
    let destination = [29, 63, 193, 255].repeat(1024);
    let actual = render(&request, Cow::Borrowed(&destination), None).unwrap();
    assert_eq!(&actual[..4], &[0; 4]);
    assert_eq!(&actual[4..], &destination[4..]);
}

#[test]
fn region_clip_uses_bitmap_origin_and_stride_for_fill_and_clear() {
    let bits: Vec<_> = (0..512).map(|i| (i * 73 + 19) as u8).collect();
    let args = [
        JsValue::Bytes(bits.clone()),
        JsValue::from(64),
        JsValue::from(64),
        JsValue::from(5),
        JsValue::from(7),
    ];
    let clip = super::super::raster_clip::Clip::from_args(&args, 0, [32, 32])
        .unwrap()
        .unwrap();
    for clear in [false, true] {
        let mut request = request();
        request.left = 5;
        request.top = 7;
        request.rect = [5.0, 7.0, 32.0, 32.0];
        request.opacity = 1.0;
        request.color = if clear {
            None
        } else {
            Some([255.0, 0.0, 0.0, 255.0])
        };
        let original = [29, 63, 193, 255].repeat(1024);
        let actual = render(&request, Cow::Borrowed(&original), Some(&clip)).unwrap();
        for (index, pixel) in actual.chunks_exact(4).enumerate() {
            let bit = (index / 32 + 7) * 64 + index % 32 + 5;
            let expected = if bits[bit / 8] & (1 << (bit % 8)) == 0 {
                [29, 63, 193, 255]
            } else if clear {
                [0; 4]
            } else {
                [255, 0, 0, 255]
            };
            assert_eq!(pixel, expected, "clear {clear}, index {index}");
        }
    }
}

#[test]
fn invalid_geometry_paint_storage_and_opaque_claims_decline_atomically() {
    let original = [29, 63, 193, 127].repeat(1024);
    for case in 0..12 {
        let mut request = request();
        match case {
            0 => request.width = 0,
            1 => request.width = u32::MAX,
            2 => request.left = u32::MAX,
            3 => request.rect[0] = f64::NAN,
            4 => request.rect[2] = 0.0,
            5 => request.matrix = [0.0; 6],
            6 => request.matrix[0] = f64::INFINITY,
            7 => request.matrix[0] = 16385.0,
            8 => request.color.as_mut().unwrap()[0] = 256.0,
            9 => request.opacity = -1.0,
            10 => request.opacity = f64::NAN,
            11 => request.opaque = true,
            _ => unreachable!(),
        }
        assert!(
            render(&request, Cow::Borrowed(&original), None).is_none(),
            "case {case}"
        );
        assert_eq!(original, [29, 63, 193, 127].repeat(1024));
    }
    for length in [0, 3, 4095, 4097] {
        assert!(render(&request(), Cow::Borrowed(&vec![0; length]), None).is_none());
    }
}

#[test]
fn malformed_bridge_packets_never_return_partial_pixels_or_change_inputs() {
    let original = vec![
        JsValue::from("canvasPaintRectangle".to_owned()),
        JsValue::from("{}".to_owned()),
        JsValue::Bytes(vec![17; 4096]),
        JsValue::Null,
    ];
    for encoded in ["{}", "null", "[]", "{\"width\":-1}", "{\"unknown\":true}"] {
        let mut args = original.clone();
        args[1] = JsValue::from(encoded.to_owned());
        let preserved = args.clone();
        assert_eq!(paint(&args), JsValue::Null);
        assert_eq!(args, preserved);
        assert_eq!(paint_owned(&mut args), JsValue::Null);
        assert_eq!(args[1], preserved[1]);
        assert_eq!(args[3], preserved[3]);
    }
}
