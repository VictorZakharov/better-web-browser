use super::*;
use kurbo::{ParamCurve, QuadBez};

fn quadratic(points: Vec<[f64; 2]>) -> Request {
    Request {
        kind: "quadratic".into(),
        points,
        arc: None,
    }
}

fn arc() -> Request {
    Request {
        kind: "arc".into(),
        points: vec![],
        arc: Some(ArcRequest {
            center: [4.0, 4.0],
            radii: [2.0, 2.0],
            start: 0.0,
            sweep: std::f64::consts::TAU,
            rotation: 0.0,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }),
    }
}

fn segment_distance(p: kurbo::Point, a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        ((p.x - a[0]) * dx + (p.y - a[1]) * dy) / length
    }
    .clamp(0.0, 1.0);
    (p.x - a[0] - t * dx).hypot(p.y - a[1] - t * dy)
}

#[test]
fn adaptive_quadratic_preserves_endpoints_and_bounds_sampled_error() {
    let points =
        flatten_request(quadratic(vec![[0.0, 0.0], [512.0, 512.0], [1024.0, 0.0]])).unwrap();
    assert_eq!(points.first(), Some(&[0.0, 0.0]));
    assert_eq!(points.last(), Some(&[1024.0, 0.0]));
    assert!(points.len() > 24 && points.len() < MAX_POINTS);
    let curve = QuadBez::new((0.0, 0.0), (512.0, 512.0), (1024.0, 0.0));
    for index in 0..=1000 {
        let p = curve.eval(index as f64 / 1000.0);
        let error = points
            .windows(2)
            .map(|pair| segment_distance(p, pair[0], pair[1]))
            .fold(f64::INFINITY, f64::min);
        assert!(error <= TOLERANCE, "{index}: {error}");
    }
}

#[test]
fn small_circles_have_real_subpixel_tessellation_and_close_numerically() {
    let points = flatten_request(arc()).unwrap();
    assert!(points.len() > 8);
    assert_eq!(points.first(), Some(&[6.0, 4.0]));
    let last = points.last().unwrap();
    assert!((last[0] - 6.0).abs() < 1e-12 && (last[1] - 4.0).abs() < 1e-12);
    for pair in points.windows(2) {
        let midpoint = [
            (pair[0][0] + pair[1][0]) / 2.0,
            (pair[0][1] + pair[1][1]) / 2.0,
        ];
        let error = (2.0 - (midpoint[0] - 4.0).hypot(midpoint[1] - 4.0)).abs();
        assert!(error <= TOLERANCE, "circle chord: {error}");
    }
}

#[test]
fn ellipse_transform_is_applied_before_adaptive_flattening() {
    let unscaled = flatten_request(arc()).unwrap();
    let mut scaled = arc();
    scaled.arc.as_mut().unwrap().transform = [20.0, 0.0, 0.0, 20.0, 0.0, 0.0];
    let scaled = flatten_request(scaled).unwrap();
    assert!(scaled.len() > unscaled.len());
    assert_eq!(scaled.first(), Some(&[120.0, 80.0]));
    for pair in scaled.windows(2) {
        let midpoint = [
            (pair[0][0] + pair[1][0]) / 2.0,
            (pair[0][1] + pair[1][1]) / 2.0,
        ];
        assert!((40.0 - (midpoint[0] - 80.0).hypot(midpoint[1] - 80.0)).abs() <= TOLERANCE);
    }
}

#[test]
fn degenerate_curves_remain_finite_and_return_their_endpoint() {
    for input in [
        vec![[1.0, 1.0]; 3],
        vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]],
    ] {
        let endpoint = *input.last().unwrap();
        let points = flatten_request(quadratic(input)).unwrap();
        assert_eq!(points.last(), Some(&endpoint));
        assert!(points.iter().flatten().all(|v| v.is_finite()));
    }
    let mut zero = arc();
    zero.arc.as_mut().unwrap().sweep = 0.0;
    assert_eq!(flatten_request(zero).unwrap(), vec![[6.0, 4.0]]);
}

#[test]
fn invalid_inputs_decline_native_work_without_unbounded_subdivision() {
    for input in [
        vec![],
        vec![[0.0, 0.0]; 4],
        vec![[f64::NAN, 0.0]; 3],
        vec![[MAX_COORDINATE + 1.0, 0.0]; 3],
    ] {
        assert!(flatten_request(quadratic(input)).is_none());
    }
    let mut invalid = arc();
    invalid.arc.as_mut().unwrap().sweep = 1e200;
    assert!(flatten_request(invalid).is_none());
    let mut invalid = arc();
    invalid.arc.as_mut().unwrap().radii = [-1.0, 2.0];
    assert!(flatten_request(invalid).is_none());
    let mut invalid = arc();
    invalid.arc.as_mut().unwrap().transform = [16384.0, 0.0, 0.0, 16384.0, 0.0, 0.0];
    assert!(flatten_request(invalid).is_none());
    assert!(matches!(
        points(&[JsValue::Null, JsValue::from("{".to_string())]),
        JsValue::Null
    ));
}
