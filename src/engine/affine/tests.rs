use super::*;

#[test]
fn composition_preserves_order_and_inverts_reflections() {
    let scale = Affine2D([-2.0, 0.0, 0.0, 3.0, 0.0, 0.0]);
    let translated = Affine2D::translate(10.0, 20.0).then_local(scale);
    assert_eq!(translated.point(4.0, 5.0), (2.0, 35.0));
    assert_eq!(translated.inverse().unwrap().point(2.0, 35.0), (4.0, 5.0));
    assert_eq!(
        scale
            .then_local(Affine2D::translate(10.0, 20.0))
            .point(4.0, 5.0),
        (-28.0, 75.0)
    );
}

#[test]
fn singular_and_nonfinite_matrices_have_no_hit_inverse() {
    for matrix in [
        Affine2D([0.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        Affine2D([1.0, 2.0, 2.0, 4.0, 0.0, 0.0]),
        Affine2D([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]),
    ] {
        assert!(matrix.inverse().is_none());
    }
}

#[test]
fn rotated_bounds_cover_all_four_corners() {
    let rotation = Affine2D([0.0, 1.0, -1.0, 0.0, 40.0, 0.0]);
    let result = rotation.bounds(RectF {
        x: 10.0,
        y: 5.0,
        width: 20.0,
        height: 10.0,
    });
    assert_eq!(
        result,
        RectF {
            x: 25.0,
            y: 10.0,
            width: 10.0,
            height: 20.0
        }
    );
}
