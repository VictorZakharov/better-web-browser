use super::*;

fn arguments(mode: &str) -> Vec<JsValue> {
    let mut source = vec![0; 4 * 4 * 4];
    source[20..24].copy_from_slice(&[200, 40, 80, 128]);
    vec![
        JsValue::Null,
        JsValue::Bytes([20, 60, 140, 192].repeat(16)),
        JsValue::Bytes(source),
        JsValue::from(mode.to_string()),
        JsValue::Null,
        JsValue::from(4.0),
        JsValue::from(4.0),
        JsValue::from(1.25),
        JsValue::from(0.5),
        JsValue::from(-0.25),
        JsValue::Bytes(vec![10, 70, 220, 160]),
    ]
}

#[test]
fn fused_transaction_matches_separate_shadow_then_source_for_all_operators() {
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
        for clip in [None, Some(vec![0b1010_1010, 0b0101_0101]), Some(vec![0; 2])] {
            let mut args = arguments(mode);
            args[4] = clip.clone().map_or(JsValue::Null, JsValue::Bytes);
            let original = args.clone();
            let destination = args[1].as_bytes().unwrap();
            let source = args[2].as_bytes().unwrap();
            let shadow =
                shadow::render_layer(source, 4, 4, 1.25, 0.5, -0.25, args[10].as_bytes().unwrap())
                    .unwrap();
            let operator = Operator::parse(mode).unwrap();
            let mut expected = destination.to_vec();
            composite_layer::composite_into(&mut expected, &shadow, operator, clip.as_deref())
                .unwrap();
            composite_layer::composite_into(&mut expected, source, operator, clip.as_deref())
                .unwrap();
            assert_eq!(render(&args).unwrap(), expected, "{mode}");
            assert_eq!(args, original, "host call must not mutate borrowed inputs");
        }
    }
}

#[test]
fn invalid_shadow_metadata_declines_without_partial_composition() {
    for (index, invalid) in [
        (3, JsValue::from("invalid".to_string())),
        (4, JsValue::Bytes(vec![])),
        (5, JsValue::from(0.0)),
        (5, JsValue::from(4.5)),
        (5, JsValue::from(f64::INFINITY)),
        (6, JsValue::from(16385.0)),
        (7, JsValue::from(-1.0)),
        (8, JsValue::from(f64::NAN)),
        (9, JsValue::Null),
        (10, JsValue::Bytes(vec![255; 3])),
    ] {
        let mut args = arguments("source-over");
        args[index] = invalid;
        let original = args[1].clone();
        assert_eq!(paint(&args), JsValue::Null, "argument {index}");
        assert_eq!(args[1], original);
    }
    assert_eq!(paint(&[]), JsValue::Null);
}

#[test]
fn copy_and_destination_in_do_not_flatten_the_shadow_with_source_over() {
    let args = arguments("copy");
    // Copy's final source operation erases the previously drawn shadow where
    // source is transparent. Flattening would wrongly retain shadow alpha.
    assert_eq!(render(&args).unwrap(), args[2].as_bytes().unwrap());
    let args = arguments("destination-in");
    let result = render(&args).unwrap();
    assert!(
        result
            .chunks_exact(4)
            .enumerate()
            .all(|(index, pixel)| index == 5 || pixel == [0; 4])
    );
    assert!(
        result[23] < 96,
        "destination sees BOTH shadow and source alpha"
    );
}

#[test]
fn transparent_shadow_is_still_an_operator_step_and_closed_clip_is_untouched() {
    let mut args = arguments("destination-in");
    args[10] = JsValue::Bytes(vec![0; 4]);
    assert_eq!(render(&args).unwrap(), [0; 64]);
    args[4] = JsValue::Bytes(vec![0; 2]);
    assert_eq!(render(&args).unwrap(), args[1].as_bytes().unwrap());
}
