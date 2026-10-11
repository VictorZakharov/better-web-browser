use super::super::raster;
use super::*;

#[test]
fn device_extents_are_checked_before_float_cast_or_pixel_allocation() {
    for value in [0., -1., f32::NAN, f32::INFINITY, f32::MAX] {
        assert!(Request::new(value, 10., 1.).is_err());
        assert!(Request::new(10., value, 1.).is_err());
        assert!(Request::new(10., 10., value).is_err());
    }
    assert!(Request::new(16384., 16384., 2.).is_err());
    assert_ne!(
        Request::new(10., 10., 1.).unwrap(),
        Request::new(10.1, 10., 1.).unwrap()
    );
}

#[test]
fn layout_viewport_override_does_not_propagate_into_nested_svg_images() {
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='40' height='20'><image href='data:image/svg+xml,%3Csvg%20xmlns=%22http://www.w3.org/2000/svg%22%20width=%2210%22%20height=%2210%22%3E%3Crect%20width=%225%22%20height=%2210%22%20fill=%22red%22/%3E%3C/svg%3E' width='20' height='20'/></svg>";
    let result = raster::render(source, &[], Request::new(80., 20., 1.).unwrap()).unwrap();
    assert_eq!(pixel(&result, 5, 10), [0, 0, 255, 255]);
    assert_eq!(pixel(&result, 15, 10), [0, 0, 0, 0]);
    assert_eq!(pixel(&result, 60, 10), [0, 0, 0, 0]);
}

#[test]
fn viewbox_origin_and_nested_percentage_geometry_are_resolved_by_the_existing_backend() {
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='20' height='20' viewBox='10 20 20 20'><rect x='10' y='20' width='20' height='20' fill='blue'/><svg x='15' y='25' width='50%' height='50%' viewBox='0 0 10 10'><rect width='50%' height='100%' fill='red'/></svg></svg>";
    for density in [1., 1.25, 2.] {
        let result = raster::render(source, &[], Request::new(40., 40., density).unwrap()).unwrap();
        assert_eq!(
            pixel(&result, (12. * density) as u32, (12. * density) as u32),
            [0, 0, 255, 255]
        );
        assert_eq!(
            pixel(&result, (25. * density) as u32, (12. * density) as u32),
            [255, 0, 0, 255]
        );
        assert_eq!(
            pixel(&result, (2. * density) as u32, (2. * density) as u32),
            [255, 0, 0, 255]
        );
    }
}

#[test]
fn fractional_css_sizes_are_not_rounded_before_vector_layout() {
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='20' height='10'><rect width='50%' height='100%' fill='red'/></svg>";
    let request = Request::new(10.25, 8.25, 1.25).unwrap();
    let result = raster::render(source, &[], request).unwrap();
    assert_eq!((result.width, result.height), (13, 11));
    assert_eq!(pixel(&result, 5, 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&result, 8, 5), [0, 0, 0, 0]);
}
