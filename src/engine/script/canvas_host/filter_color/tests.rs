use super::*;

fn request(operations: &[(&str, f64)]) -> Request {
    serde_json::from_value(serde_json::json!({"width":1,"height":1,
        "operations":operations.iter().map(|(name, value)|
            serde_json::json!({"name":name,"value":value})).collect::<Vec<_>>()
    }))
    .unwrap()
}

fn pixel(operations: &[(&str, f64)], source: [u8; 4]) -> Vec<u8> {
    render(&request(operations), Cow::Borrowed(&source)).unwrap()
}

#[test]
fn canvas_filter_color_reference_matrices_and_component_transfer_are_real() {
    let source = [53, 113, 179, 255];
    for (name, value, expected) in [
        ("brightness", 0.5, [26, 56, 90, 255]),
        ("contrast", 2.0, [0, 98, 230, 255]),
        ("invert", 1.0, [202, 142, 76, 255]),
        ("opacity", 0.5, [53, 113, 179, 128]),
        ("grayscale", 1.0, [105, 105, 105, 255]),
        ("saturate", 0.0, [105, 105, 105, 255]),
        ("sepia", 0.5, [97, 120, 139, 255]),
        (
            "hue-rotate",
            std::f64::consts::FRAC_PI_2,
            [179, 78, 157, 255],
        ),
        ("hue-rotate", std::f64::consts::PI, [157, 97, 31, 255]),
    ] {
        assert_eq!(pixel(&[(name, value)], source), expected, "{name}");
    }
}

#[test]
fn canvas_filter_color_chain_keeps_fractional_precision_but_clamps_each_primitive() {
    assert_eq!(
        pixel(&[("brightness", 0.5), ("brightness", 2.0)], [1, 3, 5, 255]),
        [1, 3, 5, 255]
    );
    // A combined identity matrix would incorrectly restore 200. The first
    // brightness clips to white before the second scales back to half-gray.
    assert_eq!(
        pixel(
            &[("brightness", 2.0), ("brightness", 0.5)],
            [200, 200, 200, 255]
        ),
        [128, 128, 128, 255]
    );
    assert_ne!(
        pixel(&[("invert", 1.0), ("sepia", 1.0)], [13, 71, 193, 255]),
        pixel(&[("sepia", 1.0), ("invert", 1.0)], [13, 71, 193, 255])
    );
}

#[test]
fn canvas_filter_color_amounts_follow_function_specific_ranges() {
    let source = [53, 113, 179, 127];
    for name in ["opacity", "invert", "grayscale", "sepia"] {
        assert_eq!(pixel(&[(name, 4.0)], source), pixel(&[(name, 1.0)], source));
    }
    assert_ne!(
        pixel(&[("saturate", 4.0)], source),
        pixel(&[("saturate", 1.0)], source)
    );
    assert_eq!(pixel(&[("brightness", 1e38)], source), [255, 255, 255, 127]);
    assert_eq!(
        pixel(&[("hue-rotate", -std::f64::consts::TAU)], source),
        source
    );
}

#[test]
fn canvas_filter_color_zero_alpha_cannot_reveal_hidden_rgb() {
    assert_eq!(pixel(&[("opacity", 0.0)], [251, 113, 179, 255]), [0; 4]);
    assert_eq!(pixel(&[("brightness", 2.0)], [251, 113, 179, 0]), [0; 4]);
    assert_eq!(
        pixel(
            &[("invert", 1.0), ("opacity", 0.0001)],
            [251, 113, 179, 127]
        ),
        [0; 4]
    );
    assert_eq!(
        pixel(&[("brightness", 1.0)], [251, 113, 179, 1]),
        [251, 113, 179, 1]
    );
}

#[test]
fn canvas_filter_color_owned_dispatch_reuses_storage_without_aliasing_borrowed_source() {
    let encoded = r#"{"width":2,"height":1,"operations":[{"name":"contrast","value":1.2}]}"#;
    let mut args = vec![
        JsValue::Null,
        JsValue::Bytes([31, 71, 113, 127].repeat(2)),
        JsValue::from(encoded.to_owned()),
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
fn canvas_filter_color_malformed_packets_decline_before_modifying_borrowed_pixels() {
    for encoded in [
        "{}",
        "[]",
        "null",
        r#"{"width":1,"height":1,"operations":[],"extra":0}"#,
        r#"{"width":1,"height":1,"operations":[{"name":"blur","value":1}]}"#,
        r#"{"width":1,"height":1,"operations":[{"name":"brightness","value":-1}]}"#,
        r#"{"width":1,"height":1,"operations":[{"name":"brightness","value":1,"extra":0}]}"#,
        r#"{"width":1,"height":1,"operations":[{"name":"brightness","value":null}]}"#,
        r#"{"width":1,"height":1,"operations":[{"name":"brightness","value":1e999}]}"#,
        r#"{"width":1,"height":1,"operations":[{"name":"brightness","value":1e308}]}"#,
        r#"{"width":1.5,"height":1,"operations":[]}"#,
    ] {
        let mut args = vec![
            JsValue::Null,
            JsValue::Bytes(vec![31, 71, 113, 127]),
            JsValue::from(encoded.to_owned()),
        ];
        let original = args.clone();
        assert_eq!(paint(&args), JsValue::Null, "{encoded}");
        assert_eq!(args, original);
        assert_eq!(paint_owned(&mut args), JsValue::Null);
        assert_eq!(args[2], original[2]);
    }
}

#[test]
fn canvas_filter_color_dimension_operation_and_work_limits_are_independent() {
    for (width, height, bytes, count) in [
        (0, 1, 4, 1),
        (1, 0, 4, 1),
        (1, 1, 3, 1),
        (u32::MAX, 1, 4, 1),
        (16385, 1, 4, 1),
        (1, 1, 4, 65),
        (1024, 1024, 1024 * 1024 * 4, 33),
    ] {
        let mut request = request(&vec![("brightness", 1.0); count]);
        request.width = width;
        request.height = height;
        assert!(render(&request, Cow::Owned(vec![0; bytes])).is_none());
    }
    let mut request = request(&[("brightness", 1.0)]);
    request.width = 2;
    assert!(render(&request, Cow::Borrowed(&[0; 4])).is_none());
}

#[test]
fn canvas_filter_color_empty_chain_is_identity_and_ties_round_to_even() {
    assert_eq!(pixel(&[], [31, 71, 113, 0]), [31, 71, 113, 0]);
    assert_eq!(
        pixel(&[("brightness", 0.5)], [1, 3, 5, 255]),
        [0, 2, 2, 255]
    );
    let source = vec![31, 71, 113, 127];
    let pointer = source.as_ptr();
    let output = render(&request(&[]), Cow::Owned(source)).unwrap();
    assert_eq!(output.as_ptr(), pointer);
}
