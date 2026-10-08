//! Owned dispatch is an allocation change, not a new drawing/policy contract.
use super::*;

fn bytes(color: [u8; 4], pixels: usize) -> JsValue {
    JsValue::Bytes(color.repeat(pixels))
}

fn arguments(operation: &str) -> (Vec<JsValue>, usize) {
    let background = bytes([31, 71, 113, 127], 16);
    let foreground = bytes([197, 43, 239, 128], 16);
    let shape = r#"{"width":4,"height":4,"left":0,"top":0,"rule":"nonzero","parts":[{"points":[[0.5,0.25],[3.75,0.5],[2.5,3.75]],"closed":true}]}"#;
    let name = JsValue::from(operation.to_owned());
    match operation {
        "canvasPaintSolidMask" => (
            vec![
                name,
                JsValue::Bytes((0..16).map(|i| (i * 17) as u8).collect()),
                background,
                JsValue::Array([197.5, 43.25, 239., 128.].map(JsValue::from).into()),
                JsValue::from(0.37),
                JsValue::Null,
            ],
            2,
        ),
        "canvasPaintGradientMask" => (
            vec![
                name,
                JsValue::from(
                    serde_json::json!({
                        "width":4,"height":4,"left":0,"top":0,
                        "transform":[1,0,0,1,0,0],"kind":"linear",
                        "geometry":[0.5,0,3.5,0],"stops":[
                            {"offset":0,"channels":[197.5,43.25,239,128]},
                            {"offset":1,"channels":[11,71,173,255]}],"opacity":0.37
                    })
                    .to_string(),
                ),
                background,
                JsValue::Null,
                JsValue::Null,
            ],
            2,
        ),
        "canvasPaintPatternMask" => (
            vec![
                name,
                JsValue::from(
                    serde_json::json!({
                        "width":4,"height":4,"left":0,"top":0,
                        "source_width":4,"source_height":4,
                        "transform":[1,0,0,1,0,0],"opacity":0.37
                    })
                    .to_string(),
                ),
                background,
                JsValue::Null,
                foreground,
                JsValue::Null,
            ],
            2,
        ),
        "canvasPaintGlyphs" => (
            vec![
                name,
                JsValue::from(
                    serde_json::json!({
                        "width":4,"height":4,"region":[0,0,4,4],
                        "inverse":[1,0,0,1,0,0],"scale":1,"opacity":0.37,
                        "operator":"source-over","paint":[197.5,43.25,239,128],
                        "opaque":false,"stroke":false
                    })
                    .to_string(),
                ),
                background,
                JsValue::Array(vec![JsValue::Array(vec![
                    JsValue::from(0),
                    JsValue::from(0),
                    JsValue::from(4),
                    JsValue::from(4),
                    JsValue::Array([0, 0, 4, 4].map(JsValue::from).into()),
                    JsValue::Boolean(false),
                    JsValue::Bytes((0..16).map(|i| (i * 17) as u8).collect()),
                ])]),
                JsValue::Null,
            ],
            2,
        ),
        "canvasPaintRectangle" => (
            vec![
                name,
                JsValue::from(
                    serde_json::json!({"width":4,"height":4,
                "left":0,"top":0,"rect":[0.25,0.5,3.25,2.75],
                "matrix":[1,0,0,1,0,0],"color":[197.5,43.25,239.0,128.0],
                "opacity":0.37,"opaque":false})
                    .to_string(),
                ),
                background,
                JsValue::Null,
            ],
            2,
        ),
        "canvasCompositeLayer" => (
            vec![
                name,
                background,
                foreground,
                JsValue::from("source-in".to_owned()),
                JsValue::Null,
                JsValue::Boolean(false),
            ],
            1,
        ),
        "canvasPaintSourceLayer" => (
            vec![
                name,
                background,
                foreground,
                JsValue::from("source-over".to_owned()),
                JsValue::Null,
                JsValue::from(4),
                JsValue::from(4),
                JsValue::from(2.5),
                JsValue::from(0.5),
                JsValue::from(-0.25),
                bytes([10, 70, 220, 160], 1),
                JsValue::Boolean(false),
            ],
            1,
        ),
        "canvasPaintShadowPath" => (
            vec![
                name,
                background,
                JsValue::from(shape.to_owned()),
                JsValue::from("source-over".to_owned()),
                JsValue::Null,
                JsValue::from(4),
                JsValue::from(4),
                JsValue::from(2.5),
                JsValue::from(0.5),
                JsValue::from(-0.25),
                bytes([10, 70, 220, 160], 1),
                JsValue::Boolean(false),
                JsValue::from("fill".to_owned()),
                JsValue::Array([197.5, 43.25, 239., 128.].map(JsValue::from).into()),
                JsValue::from(0.37),
            ],
            1,
        ),
        "canvasPaintSolidPath" => (
            vec![
                name,
                JsValue::from("fill".to_owned()),
                JsValue::from(shape.to_owned()),
                background,
                JsValue::Array([197.5, 43.25, 239., 128.].map(JsValue::from).into()),
                JsValue::from(0.37),
            ],
            3,
        ),
        "canvasPaintImage" => (
            vec![name, JsValue::from(serde_json::json!({
            "width":4,"height":4,"source_width":4,"source_height":4,
            "bounds":[0,0,4,4],"inverse":[1,0,0,1,0,0],"source":[0,0,4,4],
            "destination":[0,0,4,4],"opacity":0.37,"smooth":true,"operator":"source-over"
        }).to_string()), background, foreground, JsValue::Null],
            2,
        ),
        _ => panic!("unsupported test painter {operation}"),
    }
}

fn borrowed(operation: &str, args: &[JsValue]) -> JsValue {
    match operation {
        "canvasCompositeLayer" => composite_layer::paint(args),
        "canvasPaintSourceLayer" => source_layer::paint(args),
        "canvasPaintShadowPath" => shadow_path::paint(args),
        "canvasPaintSolidPath" => solid_path::paint(args),
        "canvasPaintImage" => image_paint::paint(args),
        "canvasPaintRectangle" => rectangle::paint(args),
        "canvasPaintSolidMask" => solid_mask::paint(args),
        "canvasPaintGradientMask" => gradient_mask::paint(args),
        "canvasPaintPatternMask" => pattern_mask::paint(args),
        "canvasPaintGlyphs" => glyph_paint::paint(args),
        _ => unreachable!(),
    }
}

const OPERATIONS: &[&str] = &[
    "canvasCompositeLayer",
    "canvasPaintSourceLayer",
    "canvasPaintShadowPath",
    "canvasPaintSolidPath",
    "canvasPaintImage",
    "canvasPaintRectangle",
    "canvasPaintSolidMask",
    "canvasPaintGradientMask",
    "canvasPaintPatternMask",
    "canvasPaintGlyphs",
];

#[test]
fn every_owned_painter_matches_borrowed_bytes_and_reuses_only_destination_storage() {
    for operation in OPERATIONS {
        let (mut args, index) = arguments(operation);
        let original = args.clone();
        let expected = borrowed(operation, &args);
        assert!(matches!(expected, JsValue::Bytes(_)), "{operation}");
        assert_eq!(args, original, "borrowed {operation}");
        let pointer = args[index].as_bytes().unwrap().as_ptr();
        let actual = painter(operation).unwrap()(&mut args);
        assert_eq!(actual, expected, "{operation}");
        assert_eq!(actual.as_bytes().unwrap().as_ptr(), pointer, "{operation}");
        for (position, (actual, original)) in args.iter().zip(&original).enumerate() {
            assert_eq!(
                actual,
                if position == index {
                    &JsValue::Null
                } else {
                    original
                },
                "{operation} argument {position}"
            );
        }
    }
}

#[test]
fn malformed_destinations_decline_without_consuming_other_inputs() {
    for operation in OPERATIONS {
        let (valid, index) = arguments(operation);
        for replacement in [
            JsValue::Null,
            JsValue::Undefined,
            JsValue::from(1),
            JsValue::from("pixels".to_owned()),
            JsValue::Array(vec![]),
            JsValue::Bytes(vec![]),
            JsValue::Bytes(vec![13; 63]),
        ] {
            let mut args = valid.clone();
            args[index] = replacement;
            let preserved = args.clone();
            assert_eq!(
                painter(operation).unwrap()(&mut args),
                JsValue::Null,
                "{operation}"
            );
            for position in 0..args.len() {
                if position != index {
                    assert_eq!(args[position], preserved[position]);
                }
            }
        }
        for length in 0..valid.len() {
            let expected = borrowed(operation, &valid[..length]);
            let mut prefix = valid[..length].to_vec();
            assert_eq!(
                painter(operation).unwrap()(&mut prefix),
                expected,
                "{operation} prefix {length}"
            );
        }
    }
}

#[test]
fn registry_and_policy_operations_cannot_enter_stateless_owned_dispatch() {
    for operation in [
        "",
        "canvasTextShape",
        "canvasTextEnvironment",
        "webglCreate",
        "webglCommand",
        "canvasEncode",
        "setAttribute",
        "cryptoRandomBytes",
        "canvasPaintImage\0",
        "canvasPaintImageSuffix",
        "CanvasPaintImage",
    ] {
        assert!(painter(operation).is_none(), "{operation}");
    }
}

#[test]
fn argument_rejections_are_identical_before_and_after_destination_consumption() {
    for operation in OPERATIONS {
        let (valid, destination_index) = arguments(operation);
        // Exercise each transport slot independently, not just a malformed
        // destination. Some optional fields accept null; compare that contract
        // too rather than assuming every substituted packet must decline.
        for index in 1..valid.len() {
            for replacement in [
                JsValue::Null,
                JsValue::Undefined,
                JsValue::Boolean(true),
                JsValue::from(f64::NAN),
                JsValue::from(-1),
                JsValue::from("{}".to_owned()),
                JsValue::Array(vec![]),
                JsValue::Bytes(vec![]),
                JsValue::Bytes(vec![255; 1]),
            ] {
                let mut args = valid.clone();
                args[index] = replacement;
                let original = args.clone();
                let expected = borrowed(operation, &args);
                let actual = painter(operation).unwrap()(&mut args);
                assert_eq!(actual, expected, "{operation} slot {index}");
                for position in 0..args.len() {
                    if position != destination_index {
                        // NaN is intentionally not PartialEq to itself. Debug
                        // equality here also checks unchanged nested packets.
                        assert_eq!(
                            format!("{:?}", args[position]),
                            format!("{:?}", original[position]),
                            "{operation} slot {index} preserved {position}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn returned_pixels_never_alias_the_other_owned_byte_inputs() {
    for operation in OPERATIONS {
        let (mut args, destination_index) = arguments(operation);
        let original = args.clone();
        let JsValue::Bytes(mut result) = painter(operation).unwrap()(&mut args) else {
            panic!("valid {operation} declined");
        };
        // Ownership reuse is restricted to destination. Modifying the result
        // cannot change retained clipping, pattern, image, or glyph packets.
        result.fill(17);
        for (index, argument) in args.iter().enumerate() {
            if index != destination_index {
                assert_eq!(argument, &original[index], "{operation} input {index}");
            }
        }
    }
}
