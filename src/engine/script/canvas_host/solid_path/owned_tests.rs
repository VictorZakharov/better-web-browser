use super::*;

fn arguments(kind: &str) -> Vec<JsValue> {
    let fields = if kind == "fill" {
        r#""rule":"nonzero""#
    } else {
        r#""line_width":2,"miter_limit":10,"cap":"round","join":"round","transform":[1,0,0,1,0,0],"dash":[],"dash_offset":0,"antialias":true"#
    };
    vec![
        JsValue::from("canvasPaintSolidPath".to_owned()),
        JsValue::from(kind.to_owned()),
        JsValue::from(format!(
            r#"{{"width":24,"height":24,"left":0,"top":0,{fields},"parts":[{{"points":[[1.25,1.5],[21,2],[9,19]],"closed":true}}]}}"#
        )),
        JsValue::Bytes([17, 39, 81, 127].repeat(24 * 24)),
        JsValue::Array([197.5, 43.25, 239.0, 128.0].map(JsValue::from).into()),
        JsValue::from(0.37),
    ]
}

#[test]
fn owned_fill_and_stroke_keep_the_allocation_and_match_borrowed_pixels() {
    for kind in ["fill", "stroke"] {
        let mut args = arguments(kind);
        let expected = paint(&args);
        let pointer = args[3].as_bytes().unwrap().as_ptr();
        let JsValue::Bytes(actual) = paint_owned(&mut args) else {
            panic!("valid owned paint rejected");
        };
        assert_eq!(actual.as_ptr(), pointer);
        assert_eq!(JsValue::Bytes(actual), expected);
        assert_eq!(args[3], JsValue::Null);
        // Geometry and color inputs stay available and unchanged.
        assert_eq!(args[2], arguments(kind)[2]);
        assert_eq!(args[4], arguments(kind)[4]);
    }
}

#[test]
fn owned_rejection_never_returns_partial_pixels() {
    for (index, invalid) in [
        (1, JsValue::from("unknown".to_owned())),
        (2, JsValue::from("{}".to_owned())),
        (3, JsValue::Bytes(vec![0; 23])),
        (4, JsValue::Array(vec![JsValue::from(f64::NAN); 4])),
        (5, JsValue::from(f64::INFINITY)),
        (6, JsValue::Bytes(vec![0xff])),
    ] {
        let mut args = arguments("fill");
        args.resize(7, JsValue::Null);
        args[index] = invalid;
        assert_eq!(paint_owned(&mut args), JsValue::Null, "argument {index}");
    }
    for count in 0..6 {
        let mut args = arguments("fill");
        args.truncate(count);
        assert_eq!(paint_owned(&mut args), JsValue::Null);
    }
}

#[test]
fn borrowed_destination_is_unchanged_and_does_not_alias_result() {
    let args = arguments("stroke");
    let before = args[3].clone();
    let JsValue::Bytes(mut painted) = paint(&args) else {
        panic!("valid borrowed paint rejected");
    };
    assert_ne!(painted.as_ptr(), args[3].as_bytes().unwrap().as_ptr());
    painted.fill(251);
    assert_eq!(args[3], before);
}

#[test]
fn rejected_in_place_composites_leave_all_destination_bytes_unchanged() {
    for (mask, color, opacity) in [
        (Some(vec![255; 23]), [0.0; 4], 1.0),
        (None, [256.0, 0.0, 0.0, 255.0], 1.0),
        (None, [0.0; 4], -0.01),
        (None, [0.0; 4], f64::NAN),
    ] {
        let before = [17, 39, 81, 127].repeat(24);
        let mut destination = before.clone();
        assert!(
            super::super::solid_mask::composite_in_place(
                mask.as_deref(),
                &mut destination,
                color,
                opacity,
                None,
            )
            .is_none()
        );
        assert_eq!(destination, before);
    }
}

#[test]
fn owned_paint_preserves_the_packed_clip_and_reuses_destination_storage() {
    let mut args = arguments("fill");
    let clip: Vec<u8> = (0..72)
        .map(|index| if index % 2 == 0 { 0x55 } else { 0xaa })
        .collect();
    args.extend([
        JsValue::Bytes(clip.clone()),
        JsValue::from(24),
        JsValue::from(24),
        JsValue::from(0),
        JsValue::from(0),
    ]);
    let expected = paint(&args);
    let pointer = args[3].as_bytes().unwrap().as_ptr();
    let JsValue::Bytes(actual) = paint_owned(&mut args) else {
        panic!("valid clip rejected");
    };
    assert_eq!(actual.as_ptr(), pointer);
    assert_eq!(JsValue::Bytes(actual), expected);
    assert_eq!(args[6], JsValue::Bytes(clip));
}
