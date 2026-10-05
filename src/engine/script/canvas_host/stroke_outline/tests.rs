use super::*;
use serde_json::json;

fn request(dash: &[f32], cap: &str, offset: f32) -> Request {
    let mut request: Request = serde_json::from_value(json!({
        "width":64,"height":64,"left":0,"top":0,
        "line_width":4,"miter_limit":10,"cap":cap,"join":"miter",
        "dash":[],"dash_offset":0,
        "parts":[{"points":[[8,20],[48,20]],"closed":false}]
    }))
    .unwrap();
    request.dash = dash.iter().map(|value| f64::from(*value)).collect();
    request.dash_offset = f64::from(offset);
    request
}

#[test]
fn dash_end_caps_are_geometry_not_centerline_visibility() {
    for cap in ["butt", "round", "square"] {
        let outline = build(&request(&[8.0, 8.0], cap, 0.0)).unwrap();
        assert!(contains(&outline, (12.0, 20.0)));
        assert!(!contains(&outline, (20.0, 20.0)));
        assert_eq!(contains(&outline, (17.0, 20.0)), cap != "butt", "{cap}");
        assert_eq!(contains(&outline, (17.5, 21.5)), cap == "square", "{cap}");
        assert!(
            contains(&outline, (12.0, 22.0)),
            "stroke boundary is included"
        );
    }
}

#[test]
fn dash_phase_resets_for_each_subpath_and_negative_offsets_wrap() {
    let mut req = request(&[8.0, 8.0], "butt", 0.0);
    req.parts[0].points[1] = [18.0, 20.0];
    req.parts.push(super::super::path::Part {
        points: vec![[8.0, 40.0], [48.0, 40.0]],
        closed: false,
    });
    let outline = build(&req).unwrap();
    assert!(contains(&outline, (10.0, 20.0)));
    assert!(
        contains(&outline, (10.0, 40.0)),
        "phase cannot carry the first subpath's length"
    );
    assert!(!contains(&outline, (20.0, 40.0)));
    let negative = build(&request(&[8.0, 8.0], "butt", -4.0)).unwrap();
    let positive = build(&request(&[8.0, 8.0], "butt", 12.0)).unwrap();
    for x in 0..60 {
        assert_eq!(
            contains(&negative, (x as f64 + 0.5, 20.0)),
            contains(&positive, (x as f64 + 0.5, 20.0))
        );
    }
}

#[test]
fn gaps_remove_original_corner_joins_but_continuous_dashes_keep_them() {
    let mut gap = request(&[8.0, 24.0], "butt", 0.0);
    gap.parts[0].points = vec![[8.0, 20.0], [24.0, 20.0], [24.0, 44.0]];
    let outline = build(&gap).unwrap();
    assert!(
        !contains(&outline, (25.0, 19.0)),
        "a gap cannot retain the original miter join"
    );
    gap.dash = vec![24.0, 8.0];
    let outline = build(&gap).unwrap();
    assert!(
        contains(&outline, (25.0, 19.0)),
        "continuous on run retains its miter join"
    );
}

#[test]
fn all_zero_dash_is_solid_and_zero_on_round_runs_have_caps() {
    let zero = build(&request(&[0.0, 0.0], "butt", 0.0)).unwrap();
    let solid = build(&request(&[], "butt", 0.0)).unwrap();
    for x in 0..60 {
        assert_eq!(
            contains(&zero, (x as f64 + 0.5, 20.0)),
            contains(&solid, (x as f64 + 0.5, 20.0))
        );
    }
    let dots = build(&request(&[0.0, 8.0], "round", 0.0)).unwrap();
    assert!(contains(&dots, (8.0, 20.0)));
    assert!(contains(&dots, (17.0, 20.0)));
    assert!(!contains(&dots, (12.0, 20.0)));
}

#[test]
fn dashing_precedes_affine_pen_transform_without_mutating_input() {
    let mut req = request(&[8.0, 8.0], "round", 0.0);
    req.transform = [2.0, 0.0, 0.0, 3.0, 0.0, 0.0];
    let outline = build(&req).unwrap();
    assert!(contains(&outline, (34.0, 60.0)));
    assert!(!contains(&outline, (40.0, 60.0)));
    assert!(contains(&outline, (24.0, 65.0)));
    assert!(!contains(&outline, (24.0, 67.0)));
    assert_eq!(req.parts[0].points, vec![[8.0, 20.0], [48.0, 20.0]]);
}

#[test]
fn malformed_and_excessive_dash_expansion_reject_before_rasterization() {
    for dash in [
        vec![-1.0, 2.0],
        vec![f32::NAN, 2.0],
        vec![1.0],
        vec![1.0; 1026],
        vec![0.00001, 0.00001],
    ] {
        assert!(build(&request(&dash, "round", 0.0)).is_none(), "{dash:?}");
    }
    assert!(build(&request(&[8.0, 8.0], "round", f32::INFINITY)).is_none());
    let mut empty = request(&[8.0, 8.0], "round", 0.0);
    empty.parts[0].points = vec![[8.0, 20.0]; 3];
    assert!(
        matches!(build(&empty), Some(Outline::Empty)),
        "prune all zero-length original lines"
    );
}

#[test]
fn valid_empty_strokes_are_distinct_from_unsupported_requests() {
    let mut req = request(&[8.0, 8.0], "round", 8.0);
    req.parts[0].points[1] = [9.0, 20.0];
    let outline = build(&req).unwrap();
    assert!(matches!(outline, Outline::Empty));
    assert!(!contains(&outline, (8.5, 20.0)));
    let mut tiny = request(&[], "butt", 0.0);
    tiny.dash = vec![1e-300, 1.0];
    assert!(
        build(&tiny).is_none(),
        "do not narrow a positive on run to zero"
    );
}

#[test]
fn closed_contours_join_continuous_dashes_across_the_seam_not_across_gaps() {
    let mut req = request(&[24.0, 8.0], "butt", 8.0);
    req.parts[0].points = vec![[8.0, 8.0], [24.0, 8.0], [24.0, 24.0], [8.0, 24.0]];
    req.parts[0].closed = true;
    let outline = build(&req).unwrap();
    assert!(
        contains(&outline, (6.5, 6.5)),
        "continuous seam retains miter join"
    );
    req.dash = vec![8.0, 24.0];
    let outline = build(&req).unwrap();
    assert!(!contains(&outline, (6.5, 6.5)), "off run removes seam join");
}
