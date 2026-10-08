use super::*;

#[test]
fn unexpected_nonconvex_overflow_declines_instead_of_dropping_vertices() {
    let mut points = [[0.0; 2]; 12];
    for (index, point) in points.iter_mut().enumerate() {
        *point = if index % 2 == 0 {
            [-1.0, 0.5]
        } else {
            [2.0, 0.5]
        };
    }
    let polygon = Polygon { points, length: 12 };
    assert!(polygon.clipped(0, 0.0, true).is_none());
    let mut full = Polygon { points, length: 12 };
    assert!(full.push([0.0, 0.0]).is_none());
    assert_eq!(full.length, 12);
}

#[test]
fn transformed_area_keeps_subpixel_edges_shear_and_reflection() {
    for transform in [
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 1.0, 1.0, 0.0],
        [0.0, 1.0, -1.0, 0.0, 1.0, 0.0],
    ] {
        let points = polygon([0.25, 0.25, 0.5, 0.5], transform);
        assert_eq!(area(&points, 0.0, 0.0), Some(0.25));
        assert_eq!(area(&points, 1.0, 0.0), Some(0.0));
    }
    let shear = polygon([0.0, 0.0, 1.0, 1.0], [1.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
    assert_eq!(area(&shear, 0.0, 0.0), Some(0.5));
    assert_eq!(area(&shear, 1.0, 0.0), Some(0.5));
}

#[test]
fn area_is_stable_when_pixel_and_shape_share_large_translation() {
    let points = polygon([0.25, 0.25, 0.5, 0.5], [1.0, 0.0, 0.0, 1.0, 8192.0, 8192.0]);
    assert_eq!(area(&points, 8192.0, 8192.0), Some(0.25));
    assert_eq!(bounds(&points), [8192.25, 8192.25, 8192.75, 8192.75]);
}

#[test]
fn clipping_uses_bounded_convex_storage_even_at_coincident_edges() {
    for angle in 0..360 {
        let angle = (angle as f64).to_radians();
        let (sin, cos) = angle.sin_cos();
        let points = polygon([-2.0, -2.0, 4.0, 4.0], [cos, sin, -sin, cos, 0.5, 0.5]);
        assert!((area(&points, 0.0, 0.0).unwrap() - 1.0).abs() < 1e-12);
    }
    for extent in [0.0, 1.0, 2.0, 16384.0] {
        let points = polygon([0.0, 0.0, extent, extent], [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert_eq!(area(&points, 0.0, 0.0), Some(extent.min(1.0).powi(2)));
    }
}
