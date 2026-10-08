//! Transport changes must not change clipped pixels, mutate borrowed inputs,
//! or return partial destination data on a malformed packet.
use super::*;

fn arguments(kind: &str, packed: bool) -> Vec<JsValue> {
    let parts = serde_json::json!([{"closed":true,"points":[[2.25,3.5],[21.0,4.25],[13.5,21.75]]}]);
    let mut request = serde_json::json!({"width":24,"height":24,"left":7,"top":9,
        "parts":if packed {serde_json::json!([])} else {parts}});
    if kind == "fill" {
        request["rule"] = serde_json::json!("nonzero");
    } else {
        for (name, value) in [
            ("line_width", serde_json::json!(2.75)),
            ("miter_limit", serde_json::json!(10)),
            ("cap", serde_json::json!("round")),
            ("join", serde_json::json!("bevel")),
            ("transform", serde_json::json!([1, 0.1, -0.2, 1, 11, 12])),
            ("dash", serde_json::json!([2.5, 1.25])),
            ("dash_offset", serde_json::json!(-0.5)),
            ("antialias", serde_json::json!(true)),
        ] {
            request[name] = value;
        }
    }
    let mut clip = vec![0u8; (41usize * 43).div_ceil(8)];
    for y in 11..29 {
        for x in 9..25 {
            let bit = y * 41 + x;
            clip[bit / 8] |= 1 << (bit % 8);
        }
    }
    let mut args = vec![
        JsValue::Null,
        JsValue::from(kind.to_owned()),
        JsValue::from(request.to_string()),
        JsValue::Bytes([31, 71, 113, 127].repeat(24 * 24)),
        JsValue::Array([197.5, 43.25, 239., 128.].map(JsValue::from).into()),
        JsValue::from(0.37),
        JsValue::Bytes(clip),
        JsValue::from(41.),
        JsValue::from(43.),
        JsValue::from(7.),
        JsValue::from(9.),
    ];
    if packed {
        args.push(JsValue::Bytes(super::super::packed_geometry::encode(&[(
            true,
            vec![[2.25, 3.5], [21., 4.25], [13.5, 21.75]],
        )])));
    }
    args
}

#[test]
fn packed_fill_and_stroke_match_json_for_borrowed_owned_and_clipped_paint() {
    for kind in ["fill", "stroke"] {
        let expected = paint(&arguments(kind, false));
        assert!(matches!(expected, JsValue::Bytes(_)));
        let mut args = arguments(kind, true);
        let source = args[3].clone();
        assert_eq!(paint(&args), expected);
        assert_eq!(args[3], source);
        let pointer = args[3].as_bytes().unwrap().as_ptr();
        let actual = paint_owned(&mut args);
        assert_eq!(actual, expected);
        assert_eq!(actual.as_bytes().unwrap().as_ptr(), pointer);
        assert_eq!(args[3], JsValue::Null);
    }
}

#[test]
fn malformed_geometry_is_rejected_atomically_even_after_a_cached_valid_draw() {
    for kind in ["fill", "stroke"] {
        let valid = arguments(kind, true);
        assert!(matches!(paint(&valid), JsValue::Bytes(_)));
        let packet = valid[11].as_bytes().unwrap();
        for end in 0..packet.len() {
            let mut args = valid.clone();
            args[11] = JsValue::Bytes(packet[..end].to_vec());
            assert_eq!(paint(&args), JsValue::Null, "{kind} prefix {end}");
            assert_eq!(args[3], valid[3]);
            assert_eq!(paint_owned(&mut args), JsValue::Null);
        }
        for invalid in [JsValue::Null, JsValue::from(1.), JsValue::Array(vec![])] {
            let mut args = valid.clone();
            args[11] = invalid;
            assert_eq!(paint(&args), JsValue::Null);
            assert_eq!(args[3], valid[3]);
        }
    }
}
