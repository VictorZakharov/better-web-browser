use super::*;

const MODES: &[&str] = &[
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
];

fn compact(bounds: [u32; 4]) -> shadow::Layer {
    let mut pixels = Vec::new();
    for y in bounds[1]..bounds[3] {
        for x in bounds[0]..bounds[2] {
            pixels.extend([
                (x * 17 + y * 31) as u8,
                (x * 43 + y * 3) as u8,
                (x * 5 + y * 79) as u8,
                [0, 1, 64, 128, 254, 255][(x + y) as usize % 6],
            ]);
        }
    }
    shadow::Layer { bounds, pixels }
}

fn expanded(layer: &shadow::Layer, width: u32, height: u32) -> Vec<u8> {
    let mut dense = vec![0; width as usize * height as usize * 4];
    let [left, top, right, bottom] = layer.bounds;
    let length = (right - left) as usize * 4;
    if length != 0 {
        for (y, row) in (top..bottom).zip(layer.pixels.chunks_exact(length)) {
            let start = (y as usize * width as usize + left as usize) * 4;
            dense[start..start + length].copy_from_slice(row);
        }
    }
    dense
}

fn args(dense: Vec<u8>, mode: &str, alpha: u8, clip: Option<Vec<u8>>) -> Vec<JsValue> {
    vec![
        JsValue::Null,
        JsValue::Bytes([31, 79, 137, alpha].repeat(24 * 19)),
        JsValue::Bytes(dense),
        JsValue::from(mode.to_owned()),
        clip.map_or(JsValue::Null, JsValue::Bytes),
        JsValue::from(24.0),
        JsValue::from(19.0),
        JsValue::from(3.125),
        JsValue::from(1.75),
        JsValue::from(-0.625),
        JsValue::Bytes(vec![17, 73, 241, 197]),
        JsValue::Boolean(false),
    ]
}

#[test]
fn compact_source_and_shadow_match_dense_for_every_operator_clip_and_storage_alpha() {
    for bounds in [[0, 0, 24, 19], [3, 5, 19, 14], [23, 18, 24, 19], [0; 4]] {
        let source = compact(bounds);
        let dense = expanded(&source, 24, 19);
        for mode in MODES {
            for alpha in [0, 127, 255] {
                for clip in [None, Some(vec![0x96; 57]), Some(vec![0; 57])] {
                    let mut arguments = args(dense.clone(), mode, alpha, clip);
                    for opaque in [false, true] {
                        if opaque && alpha != 255 {
                            continue;
                        }
                        arguments[11] = JsValue::Boolean(opaque);
                        let original = arguments.clone();
                        assert_eq!(
                            render_region(&arguments, &source),
                            super::super::render(&arguments, None),
                            "bounds={bounds:?}, mode={mode}, alpha={alpha}, opaque={opaque}"
                        );
                        assert_eq!(
                            arguments, original,
                            "transaction changed caller-owned input"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn compact_alpha_support_retains_global_origin_padding_and_fractional_shadow_offsets() {
    for bounds in [
        [0, 0, 7, 5],
        [7, 4, 19, 15],
        [23, 18, 24, 19],
        [24, 19, 24, 19],
    ] {
        let source = compact(bounds);
        let dense = expanded(&source, 24, 19);
        for blur in [0.0, 0.005, 1.125, 5.75, 128.0, f64::MAX] {
            for offsets in [[0.0, 0.0], [0.5, -0.25], [-20.75, 17.5], [1e12, -1e12]] {
                let actual = shadow::render_region_layer(
                    &source.pixels,
                    [24, 19],
                    source.bounds,
                    blur,
                    offsets,
                    &[9, 61, 213, 129],
                )
                .unwrap();
                let expected = shadow::render_sparse_layer(
                    &dense,
                    24,
                    19,
                    blur,
                    offsets[0],
                    offsets[1],
                    &[9, 61, 213, 129],
                )
                .unwrap();
                assert_eq!(
                    actual.bounds, expected.bounds,
                    "bounds={bounds:?}, blur={blur}, offsets={offsets:?}"
                );
                assert_eq!(
                    actual.pixels, expected.pixels,
                    "bounds={bounds:?}, blur={blur}, offsets={offsets:?}"
                );
            }
        }
    }
}

#[test]
fn invalid_compact_shapes_and_options_decline_atomically_including_transparent_shadows() {
    let valid = compact([3, 5, 19, 14]);
    let mut arguments = args(expanded(&valid, 24, 19), "source-over", 255, None);
    for bounds in [[4, 3, 2, 9], [0, 0, 25, 19], [0, 0, 24, 20], [u32::MAX; 4]] {
        let invalid = shadow::Layer {
            bounds,
            pixels: valid.pixels.clone(),
        };
        let original = arguments.clone();
        assert!(render_region(&arguments, &invalid).is_none());
        arguments[10] = JsValue::Bytes(vec![0; 4]);
        assert!(render_region(&arguments, &invalid).is_none());
        arguments[10] = original[10].clone();
        assert_eq!(arguments, original);
    }
    let invalid = shadow::Layer {
        bounds: valid.bounds,
        pixels: vec![0; valid.pixels.len() - 1],
    };
    assert!(render_region(&arguments, &invalid).is_none());
    for (index, invalid) in [
        (3, JsValue::from("unknown".to_owned())),
        (4, JsValue::Bytes(vec![0; 56])),
        (5, JsValue::from(0.0)),
        (7, JsValue::from(-1.0)),
        (8, JsValue::from(f64::INFINITY)),
        (9, JsValue::from(f64::NAN)),
        (10, JsValue::Bytes(vec![255; 3])),
        (11, JsValue::from("opaque".to_owned())),
    ] {
        let mut args = arguments.clone();
        args[index] = invalid;
        let original = args.clone();
        assert!(render_region(&args, &valid).is_none());
        // NaN is intentionally unequal to itself; compare only the destination.
        assert_eq!(args[1], original[1]);
    }
    arguments[1] = JsValue::Bytes([1, 2, 3, 128].repeat(24 * 19));
    arguments[11] = JsValue::Boolean(true);
    assert!(render_region(&arguments, &valid).is_none());
}
