use super::*;

#[test]
fn matrix_fallback_does_not_collapse_a_half_rotation_into_a_zero_scale() {
    let a = parse_transform("matrix(1,0,0,1,0,0)").unwrap();
    let b = parse_transform("matrix(-1,0,0,-1,0,0)").unwrap();
    let value = interpolate(&a.0, &b.0, 0.5).unwrap();
    let matrix = TransformList(value).matrix(100.0, 80.0, 16.0);
    let (x, y) = matrix.point(10.0, 0.0);
    assert!(x.abs() < 0.00001 && (y.abs() - 10.0).abs() < 0.00001);
}

#[test]
fn decomposition_recomposes_skew_and_reflection_endpoints() {
    for matrix in [
        Affine2D([2.0, 1.0, 0.5, 3.0, 10.0, 20.0]),
        Affine2D([-2.0, 1.0, 0.5, 3.0, 10.0, 20.0]),
    ] {
        let value = interpolate(
            &[Operation::Matrix(matrix)],
            &[Operation::Matrix(matrix)],
            0.5,
        )
        .unwrap();
        let recomposed = TransformList(value).matrix(100.0, 80.0, 16.0);
        for (a, b) in matrix.0.into_iter().zip(recomposed.0) {
            assert!((a - b).abs() < 0.00001, "{matrix:?} != {recomposed:?}");
        }
    }
}

#[test]
fn relative_mixed_tails_are_not_resolved_using_a_fictitious_zero_box() {
    let a = parse_transform("translateX(50%) scale(2)").unwrap();
    let b = parse_transform("rotate(30deg)").unwrap();
    assert!(interpolate(&a.0, &b.0, 0.5).is_none());
}
