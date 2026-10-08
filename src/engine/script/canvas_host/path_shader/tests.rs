//! Fused source generation must equal the same mask/shader pipeline, including
//! packed geometry, bitmap-space clipping, hidden RGB and changed live paint.
use super::*;
use crate::engine::script::canvas_host::{
    fill, gradient_mask, packed_geometry, path, pattern_mask,
};

fn arguments(shader: &str, kind: &str, packed: bool, alpha: u8) -> Vec<JsValue> {
    let points = vec![[10.25, 10.5], [28.75, 12.25], [17.5, 32.75]];
    let mut shape = serde_json::json!({"width":32,"height":33,"left":7,"top":9,
        "parts":if packed {serde_json::json!([])} else {
            serde_json::json!([{"closed":true,"points":points}])}});
    if kind == "fill" {
        shape["rule"] = serde_json::json!("evenodd");
    } else {
        for (key, value) in [
            ("line_width", serde_json::json!(1.25)),
            ("miter_limit", serde_json::json!(10)),
            ("cap", serde_json::json!("round")),
            ("join", serde_json::json!("bevel")),
            ("dash", serde_json::json!([2.5, 1.25])),
            ("dash_offset", serde_json::json!(-0.5)),
            ("antialias", serde_json::json!(true)),
        ] {
            shape[key] = value;
        }
    }
    let transform = serde_json::json!([1, 0.125, -0.25, 1, 3.5, -2.75]);
    let mut paint = serde_json::json!({"width":32,"height":33,"left":7,"top":9,
        "transform":transform,"opacity":0.37});
    if shader == "pattern" {
        paint["source_width"] = serde_json::json!(2);
        paint["source_height"] = serde_json::json!(2);
    } else {
        paint["kind"] = serde_json::json!(shader);
        paint["geometry"] = match shader {
            "linear" => serde_json::json!([0.5, 0, 31.5, 0]),
            "radial" => serde_json::json!([16, 16, 2, 16, 16, 25]),
            "conic" => serde_json::json!([0.375, 16, 16]),
            _ => unreachable!(),
        };
        paint["stops"] = serde_json::json!([
            {"offset":0,"channels":[197,43,239,128]},
            {"offset":0.5,"channels":[11,71,173,0]},
            {"offset":0.5,"channels":[87,63,19,183]},
            {"offset":1,"channels":[219,181,113,255]}]);
    }
    let mut args = vec![
        JsValue::Null,
        JsValue::from(paint.to_string()),
        JsValue::Bytes([31, 71, 113, alpha].repeat(32 * 33)),
    ];
    if shader == "pattern" {
        args.push(JsValue::Bytes(vec![
            197, 43, 239, 128, 11, 71, 173, 0, 87, 63, 19, 183, 219, 181, 113, 255,
        ]));
    }
    args.push(JsValue::from(kind.to_owned()));
    args.push(JsValue::from(shape.to_string()));
    let bits = (0..(61usize * 53).div_ceil(8))
        .map(|i| (i * 73 + 19) as u8)
        .collect();
    args.extend([
        JsValue::Bytes(bits),
        JsValue::from(61),
        JsValue::from(53),
        JsValue::from(7),
        JsValue::from(9),
    ]);
    args.push(if packed {
        JsValue::Bytes(packed_geometry::encode(&[(true, points)]))
    } else {
        JsValue::Null
    });
    args
}

fn fused(shader: &str, args: &[JsValue]) -> JsValue {
    if shader == "pattern" {
        pattern_mask::paint_path(args)
    } else {
        gradient_mask::paint_path(args)
    }
}

fn fused_owned(shader: &str, args: &mut [JsValue]) -> JsValue {
    if shader == "pattern" {
        pattern_mask::paint_path_owned(args)
    } else {
        gradient_mask::paint_path_owned(args)
    }
}

fn separate(shader: &str, args: &[JsValue]) -> JsValue {
    let kind_index = if shader == "pattern" { 4 } else { 3 };
    let JsValue::String(kind) = &args[kind_index] else {
        unreachable!();
    };
    let JsValue::String(source) = &args[kind_index + 1] else {
        unreachable!();
    };
    let geometry = match &args[kind_index + 7] {
        JsValue::Null => None,
        value => Some(value),
    };
    let coverage = if kind == "fill" {
        fill::coverage_from_arguments(source, geometry)
    } else {
        path::coverage_from_arguments(source, geometry)
    }
    .unwrap();
    let mut separate = vec![
        JsValue::Null,
        args[1].clone(),
        args[2].clone(),
        JsValue::Bytes(std::sync::Arc::unwrap_or_clone(coverage).into_vec()),
    ];
    if shader == "pattern" {
        separate.push(args[3].clone());
    }
    separate.extend_from_slice(&args[kind_index + 2..kind_index + 7]);
    if shader == "pattern" {
        pattern_mask::paint(&separate)
    } else {
        gradient_mask::paint(&separate)
    }
}

#[test]
fn native_shader_paths_equal_separate_paints_for_geometry_clips_and_every_backdrop_alpha() {
    for shader in ["linear", "radial", "conic", "pattern"] {
        for kind in ["fill", "stroke"] {
            for alpha in [0, 1, 127, 255] {
                let json = arguments(shader, kind, false, alpha);
                let expected = separate(shader, &json);
                assert!(expected.as_bytes().is_some(), "{shader} {kind}");
                for packed in [false, true] {
                    let mut args = arguments(shader, kind, packed, alpha);
                    let original = args.clone();
                    assert_eq!(
                        fused(shader, &args),
                        expected,
                        "{shader} {kind} packed {packed}"
                    );
                    assert_eq!(args, original);
                    let pointer = args[2].as_bytes().unwrap().as_ptr();
                    let actual = fused_owned(shader, &mut args);
                    assert_eq!(actual, expected);
                    assert_eq!(actual.as_bytes().unwrap().as_ptr(), pointer);
                    assert_eq!(args[2], JsValue::Null);
                    for index in 0..args.len() {
                        if index != 2 {
                            assert_eq!(args[index], original[index]);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn geometry_paint_and_clip_regions_cannot_disagree_even_after_a_valid_cache_hit() {
    for shader in ["linear", "pattern"] {
        for kind in ["fill", "stroke"] {
            let valid = arguments(shader, kind, true, 127);
            assert!(fused(shader, &valid).as_bytes().is_some());
            let shape_index = if shader == "pattern" { 5 } else { 4 };
            for key in ["width", "height", "left", "top", "unexpected"] {
                let mut args = valid.clone();
                let JsValue::String(encoded) = &args[shape_index] else {
                    unreachable!();
                };
                let mut shape: serde_json::Value = serde_json::from_str(encoded).unwrap();
                shape[key] = serde_json::json!(17);
                args[shape_index] = JsValue::from(shape.to_string());
                let original = args.clone();
                assert_eq!(fused(shader, &args), JsValue::Null, "{shader} {kind} {key}");
                assert_eq!(args, original);
                assert_eq!(fused_owned(shader, &mut args), JsValue::Null);
                assert_eq!(args[shape_index], original[shape_index]);
            }
        }
    }
}

#[test]
fn truncated_packed_geometry_declines_without_committing_any_destination() {
    for shader in ["linear", "pattern"] {
        for kind in ["fill", "stroke"] {
            let valid = arguments(shader, kind, true, 127);
            let geometry_index = if shader == "pattern" { 11 } else { 10 };
            let bytes = valid[geometry_index].as_bytes().unwrap();
            for length in 0..valid.len() {
                let mut prefix = valid[..length].to_vec();
                assert_eq!(fused(shader, &prefix), JsValue::Null);
                assert_eq!(fused_owned(shader, &mut prefix), JsValue::Null);
            }
            for length in 0..bytes.len() {
                let mut args = valid.clone();
                args[geometry_index] = JsValue::Bytes(bytes[..length].to_vec());
                assert_eq!(
                    fused(shader, &args),
                    JsValue::Null,
                    "{shader} {kind} prefix {length}"
                );
                assert_eq!(args[2], valid[2]);
                assert_eq!(fused_owned(shader, &mut args), JsValue::Null);
                assert_eq!(args[geometry_index].as_bytes().unwrap(), &bytes[..length]);
            }
        }
    }
}

#[test]
fn changed_live_shader_values_are_not_replaced_by_cached_coverage_pixels() {
    for shader in ["linear", "pattern"] {
        let mut args = arguments(shader, "fill", true, 127);
        let first = fused(shader, &args);
        let JsValue::String(encoded) = &args[1] else {
            unreachable!();
        };
        let mut paint: serde_json::Value = serde_json::from_str(encoded).unwrap();
        paint["opacity"] = serde_json::json!(0.81);
        paint["transform"] = serde_json::json!([1, 0, 0, 1, 12.5, -7.25]);
        args[1] = JsValue::from(paint.to_string());
        let second = fused(shader, &args);
        assert_ne!(second, first, "{shader} live paint was ignored");
        assert_eq!(second, separate(shader, &args));
    }
}
