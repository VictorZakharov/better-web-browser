use super::*;

fn request(kind: &str) -> String {
    let fields = if kind == "fill" {
        r#""rule":"evenodd""#
    } else {
        r#""line_width":2.25,"miter_limit":10,"cap":"round","join":"bevel","transform":[1,0.2,0.1,1,0,0],"dash":[3,2],"dash_offset":0.5,"antialias":true"#
    };
    format!(
        r#"{{"width":20,"height":18,"left":-2,"top":-1,{fields},"parts":[{{"points":[[1.25,1],[15,2.5],[8,14]],"closed":true}}]}}"#
    )
}

fn args(
    kind: &str,
    source: String,
    destination: Vec<u8>,
    color: [f64; 4],
    opacity: f64,
) -> Vec<JsValue> {
    vec![
        JsValue::Null,
        JsValue::String(kind.into()),
        JsValue::String(source),
        JsValue::Bytes(destination),
        JsValue::Array(color.into_iter().map(JsValue::from).collect()),
        JsValue::from(opacity),
    ]
}

#[test]
fn fused_fill_and_stroke_equal_separate_coverage_and_paint_for_every_destination_alpha() {
    for kind in ["fill", "stroke"] {
        let source = request(kind);
        let mask = if kind == "fill" {
            super::super::fill::mask_from_source(&source)
        } else {
            super::super::path::mask_from_source(&source)
        }
        .unwrap();
        for alpha in [0, 1, 63, 127, 128, 254, 255] {
            for opacity in [0.0, 0.125, 0.5, 1.0] {
                let destination = [31, 71, 113, alpha].repeat(360);
                let color = [197.0, 43.0, 239.0, 128.0];
                let expected = super::super::solid_mask::composite_region(
                    Some(&mask),
                    &destination,
                    color,
                    opacity,
                )
                .unwrap();
                let JsValue::Bytes(actual) =
                    paint(&args(kind, source.clone(), destination, color, opacity))
                else {
                    panic!("valid fused request declined");
                };
                assert_eq!(
                    actual, expected,
                    "{kind} backdrop {alpha} opacity {opacity}"
                );
            }
        }
    }
}

#[test]
fn invalid_geometry_paint_and_destination_payloads_fail_closed() {
    let valid = args(
        "fill",
        request("fill"),
        [1, 2, 3, 4].repeat(360),
        [1.0, 2.0, 3.0, 4.0],
        1.0,
    );
    for (index, replacement) in [
        (1, JsValue::String("rectangle".into())),
        (2, JsValue::String("{}".into())),
        (2, JsValue::String(request("fill").replace("20", "0"))),
        (2, JsValue::String(request("fill").replace("20", "20.5"))),
        (
            2,
            JsValue::String(request("fill").replace("20", "4294967295")),
        ),
        (
            2,
            JsValue::String(request("fill").replace("evenodd", "invalid")),
        ),
        (
            2,
            JsValue::String(request("fill").replace("{\"width\"", "{\"unknown\":1,\"width\"")),
        ),
        (3, JsValue::Bytes(vec![])),
        (3, JsValue::Bytes(vec![0; 1439])),
        (3, JsValue::Bytes(vec![0; 1444])),
        (4, JsValue::Array(vec![JsValue::from(255.0); 3])),
        (4, JsValue::Array(vec![JsValue::from(f64::NAN); 4])),
        (4, JsValue::Array(vec![JsValue::from(256.0); 4])),
        (4, JsValue::Array(vec![JsValue::from(-1.0); 4])),
        (5, JsValue::from(-0.1)),
        (5, JsValue::from(1.1)),
        (5, JsValue::from(f64::INFINITY)),
    ] {
        let mut candidate = valid.clone();
        candidate[index] = replacement;
        assert!(
            matches!(paint(&candidate), JsValue::Null),
            "argument {index}"
        );
    }
    for count in 0..valid.len() {
        assert!(matches!(paint(&valid[..count]), JsValue::Null));
    }
}

#[test]
fn owned_outputs_cannot_mutate_destination_or_cached_geometry() {
    let arguments = args(
        "stroke",
        request("stroke"),
        [17, 39, 81, 127].repeat(360),
        [239.0, 127.0, 23.0, 129.0],
        0.75,
    );
    let JsValue::Bytes(mut first) = paint(&arguments) else {
        panic!("first draw declined")
    };
    let original = first.clone();
    first.fill(0);
    let JsValue::Bytes(second) = paint(&arguments) else {
        panic!("cached draw declined")
    };
    assert_eq!(second, original);
    assert!(
        matches!(&arguments[3], JsValue::Bytes(destination) if *destination==[17,39,81,127].repeat(360))
    );
}
