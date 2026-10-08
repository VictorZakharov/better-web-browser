use super::*;

pub(super) fn arguments(
    kind: &str,
    mode: &str,
    opacity: f64,
    alpha: u8,
    clip: Option<Vec<u8>>,
) -> Vec<JsValue> {
    let geometry = if kind == "fill" {
        r#""rule":"evenodd""#
    } else {
        r#""line_width":2.25,"miter_limit":10,"cap":"round","join":"bevel","transform":[1,0.2,0.1,1,0,0],"dash":[3,2],"dash_offset":0.5,"antialias":true"#
    };
    vec![
        JsValue::Null,
        JsValue::Bytes([31, 71, 113, alpha].repeat(32 * 24)),
        JsValue::String(format!(
            r#"{{"width":20,"height":18,"left":2,"top":1,{geometry},"parts":[{{"points":[[3.25,2],[17,3.5],[10,15]],"closed":true}}]}}"#
        )),
        JsValue::from(mode.to_string()),
        clip.map_or(JsValue::Null, JsValue::Bytes),
        JsValue::from(32.0),
        JsValue::from(24.0),
        JsValue::from(2.5),
        JsValue::from(0.5),
        JsValue::from(-0.25),
        JsValue::Bytes(vec![10, 70, 220, 160]),
        JsValue::Boolean(false),
        JsValue::from(kind.to_string()),
        JsValue::Array(
            [197.0, 43.0, 239.0, 128.0]
                .into_iter()
                .map(JsValue::from)
                .collect(),
        ),
        JsValue::from(opacity),
    ]
}

#[test]
fn solid_geometry_transaction_matches_the_separate_mask_shadow_and_source_pipeline() {
    for kind in ["fill", "stroke"] {
        for mode in [
            "source-over",
            "source-in",
            "source-out",
            "source-atop",
            "destination-over",
            "destination-in",
            "destination-out",
            "destination-atop",
            "xor",
            "copy",
            "lighter",
            "multiply",
            "screen",
            "overlay",
            "darken",
            "lighten",
            "color-dodge",
            "color-burn",
            "hard-light",
            "soft-light",
            "difference",
            "exclusion",
            "hue",
            "saturation",
            "color",
            "luminosity",
        ] {
            for alpha in [0, 127, 255] {
                for opacity in [0.0, 0.125, 0.5, 1.0] {
                    for clip in [None, Some(vec![0x96; 96]), Some(vec![0; 96])] {
                        let args = arguments(kind, mode, opacity, alpha, clip);
                        let original = args.clone();
                        let request = args[2].string_value();
                        let mask = if kind == "fill" {
                            super::super::fill::mask_from_source(&request)
                        } else {
                            super::super::path::mask_from_source(&request)
                        }
                        .unwrap();
                        let region = super::super::solid_mask::composite_region(
                            Some(&mask),
                            &vec![0; 20 * 18 * 4],
                            [197.0, 43.0, 239.0, 128.0],
                            opacity,
                        )
                        .unwrap();
                        let mut source = vec![0; 32 * 24 * 4];
                        for (row, bytes) in region.chunks_exact(80).enumerate() {
                            let start = ((row + 1) * 32 + 2) * 4;
                            source[start..start + 80].copy_from_slice(bytes);
                        }
                        let shadow = super::super::shadow::render_layer(
                            &source,
                            32,
                            24,
                            2.5,
                            0.5,
                            -0.25,
                            &[10, 70, 220, 160],
                        )
                        .unwrap();
                        let operator = super::super::composite::Operator::parse(mode).unwrap();
                        let mut expected = args[1].as_bytes().unwrap().to_vec();
                        let clip = args[4].as_bytes();
                        super::super::composite_layer::composite_into(
                            &mut expected,
                            &shadow,
                            operator,
                            clip,
                        )
                        .unwrap();
                        super::super::composite_layer::composite_into(
                            &mut expected,
                            &source,
                            operator,
                            clip,
                        )
                        .unwrap();
                        assert_eq!(
                            render(&args).unwrap(),
                            expected,
                            "{kind} {mode} {alpha} {opacity}"
                        );
                        assert_eq!(
                            args, original,
                            "borrowed bitmap and geometry remain unchanged"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn bad_geometry_or_paint_never_returns_a_partially_painted_destination() {
    let valid = arguments("fill", "source-over", 1.0, 255, None);
    for (index, replacement) in [
        (1, JsValue::Bytes(vec![0; 3071])),
        (2, JsValue::from("{}".to_string())),
        (
            2,
            JsValue::from(valid[2].string_value().replace("\"left\":2", "\"left\":-1")),
        ),
        (
            2,
            JsValue::from(
                valid[2]
                    .string_value()
                    .replace("\"width\":20", "\"width\":4294967295"),
            ),
        ),
        (3, JsValue::from("invalid".to_string())),
        (4, JsValue::Bytes(vec![0; 95])),
        (5, JsValue::from(31.5)),
        (7, JsValue::from(-1.0)),
        (8, JsValue::from(f64::INFINITY)),
        (10, JsValue::Bytes(vec![255; 3])),
        (11, JsValue::Boolean(true)),
        (12, JsValue::from("invalid".to_string())),
        (13, JsValue::Array(vec![JsValue::from(255.0); 3])),
        (13, JsValue::Array(vec![JsValue::from(f64::NAN); 4])),
        (14, JsValue::from(1.01)),
    ] {
        let mut args = valid.clone();
        args[index] = replacement;
        // An opaque claim is valid here: the fully opaque destination remains so.
        if index == 11 {
            args[1] = JsValue::Bytes([0, 0, 0, 127].repeat(32 * 24));
        }
        let before = args.clone();
        assert!(render(&args).is_none(), "invalid argument {index}");
        assert_eq!(args[1], before[1], "destination unchanged");
        assert_eq!(args[2], before[2], "geometry unchanged");
    }
}
