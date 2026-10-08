//! Both transports converge on the same validated tiny-skia providers. This
//! compares complete coverage bytes, not a tolerance or a checksum.
use super::encode;
use crate::engine::script::JsValue;

fn compare(mut request: serde_json::Value, parts: &[(bool, Vec<[f64; 2]>)], stroke: bool) {
    request["parts"] = serde_json::json!(
        parts
            .iter()
            .map(|(closed, points)| serde_json::json!({"closed":closed,"points":points}))
            .collect::<Vec<_>>()
    );
    let json = request.to_string();
    request["parts"] = serde_json::json!([]);
    let metadata = request.to_string();
    let geometry = JsValue::Bytes(encode(parts));
    let provider = if stroke {
        super::super::path::coverage_from_arguments
    } else {
        super::super::fill::coverage_from_arguments
    };
    let original = provider(&json, None).unwrap();
    let packed = provider(&metadata, Some(&geometry)).unwrap();
    assert_eq!(original.as_slice(), packed.as_slice(), "{json}");
}

#[test]
fn fill_transport_preserves_winding_roi_subpaths_and_fractional_edges() {
    for rule in ["nonzero", "evenodd"] {
        for origin in [[0, 0], [9, 13], [-12, -5]] {
            for seed in 0..32 {
                let x = origin[0] as f64 + seed as f64 / 17.0;
                let y = origin[1] as f64 + seed as f64 / 23.0;
                let mut contour = vec![
                    [x, y],
                    [x + 20.75, y + 0.5],
                    [x + 18.5, y + 19.25],
                    [x + 0.25, y + 18.0],
                ];
                let second = if seed % 2 == 0 {
                    contour.clone()
                } else {
                    contour.reverse();
                    contour.clone()
                };
                compare(
                    serde_json::json!({"width":31,"height":29,"left":origin[0],"top":origin[1],"rule":rule}),
                    &[(false, contour), (true, second), (false, vec![[x, y]])],
                    false,
                );
            }
        }
    }
}

#[test]
fn stroke_transport_preserves_pen_dash_transform_and_antialiasing() {
    for cap in ["butt", "round", "square"] {
        for join in ["miter", "round", "bevel"] {
            for antialias in [false, true] {
                for transform in [
                    [1., 0., 0., 1., 0., 0.],
                    [1.2, 0.2, -0.1, 0.8, 3., 4.],
                    [-1., 0., 0., 1., 28., 0.],
                ] {
                    for dash in [vec![], vec![2.5, 1.25, 4., 2.]] {
                        compare(
                            serde_json::json!({"width":37,"height":33,"left":-2,"top":-3,
                            "line_width":2.75,"miter_limit":7.0,"cap":cap,"join":join,
                            "transform":transform,"dash":dash,"dash_offset":-1.75,"antialias":antialias}),
                            &[
                                (false, vec![[2.25, 3.5], [19.75, 4.25], [14.5, 24.75]]),
                                (true, vec![[5., 5.], [20., 5.], [12., 20.]]),
                            ],
                            true,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn malformed_transport_cannot_fall_back_to_empty_or_second_geometry() {
    for stroke in [false, true] {
        let provider = if stroke {
            super::super::path::coverage_from_arguments
        } else {
            super::super::fill::coverage_from_arguments
        };
        let mut metadata = serde_json::json!({"width":16,"height":16,"left":0,"top":0,"parts":[]});
        if stroke {
            for (key, value) in [
                ("line_width", serde_json::json!(1)),
                ("miter_limit", serde_json::json!(10)),
                ("cap", serde_json::json!("butt")),
                ("join", serde_json::json!("miter")),
            ] {
                metadata[key] = value;
            }
        } else {
            metadata["rule"] = serde_json::json!("nonzero");
        }
        let valid = JsValue::Bytes(encode(&[(false, vec![[1., 1.], [8., 1.], [8., 8.]])]));
        let source = metadata.to_string();
        assert!(provider(&source, Some(&valid)).is_some());
        for invalid in [
            JsValue::Null,
            JsValue::from("wrong".to_string()),
            JsValue::Bytes(vec![]),
            JsValue::Bytes(encode(&[(false, vec![[f64::INFINITY, 1.]])])),
        ] {
            assert!(provider(&source, Some(&invalid)).is_none());
        }
        metadata["parts"] = serde_json::json!([{"closed":false,"points":[[1,1]]}]);
        assert!(provider(&metadata.to_string(), Some(&valid)).is_none());
        metadata.as_object_mut().unwrap().remove("parts");
        assert!(provider(&metadata.to_string(), Some(&valid)).is_none());
        metadata["parts"] = serde_json::json!([]);
        metadata["unexpected"] = serde_json::json!(true);
        assert!(provider(&metadata.to_string(), Some(&valid)).is_none());
    }
}

#[test]
fn cache_keys_distinguish_bytes_metadata_and_legacy_transport() {
    let a = encode(&[(false, vec![[0., 0.], [8., 0.], [8., 8.]])]);
    let b = encode(&[(false, vec![[0., 0.], [4., 0.], [4., 4.]])]);
    let key = "packed geometry cache exact equality regression";
    let first = super::super::mask_cache::rasterize_packed(
        super::super::mask_cache::Kind::Fill,
        key,
        &a,
        || Some(vec![1]),
    )
    .unwrap();
    let hit = super::super::mask_cache::rasterize_packed(
        super::super::mask_cache::Kind::Fill,
        key,
        &a,
        || panic!("exact hit"),
    )
    .unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &hit));
    let second = super::super::mask_cache::rasterize_packed(
        super::super::mask_cache::Kind::Fill,
        key,
        &b,
        || Some(vec![2]),
    )
    .unwrap();
    assert_eq!(second.as_slice(), [2]);
    let legacy = super::super::mask_cache::rasterize_shared(
        super::super::mask_cache::Kind::Fill,
        key,
        || Some(vec![3]),
    )
    .unwrap();
    assert_eq!(legacy.as_slice(), [3]);
    let different = super::super::mask_cache::rasterize_packed(
        super::super::mask_cache::Kind::Stroke,
        key,
        &a,
        || Some(vec![4]),
    )
    .unwrap();
    assert_eq!(different.as_slice(), [4]);
    assert_eq!(first.as_slice(), [1]);
}
