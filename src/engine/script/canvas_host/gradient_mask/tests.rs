use super::*;

#[test]
fn host_gradient_clipping_rejects_ambiguous_origin_and_invalid_bitmap_metadata() {
    let encoded = r#"{"width":4,"height":2,"left":2,"top":1,"transform":[1,0,0,1,0,0],"kind":"linear","geometry":[0.5,0,2.5,0],"stops":[{"offset":0,"channels":[255,0,0,255]},{"offset":1,"channels":[0,0,255,255]}],"opacity":0.5}"#;
    let destination = [11, 22, 33, 255].repeat(8);
    let mut arguments = vec![
        JsValue::from("canvasPaintGradientMask".to_owned()),
        JsValue::from(encoded.to_owned()),
        JsValue::Bytes(destination.clone()),
        JsValue::Null,
        JsValue::Bytes(vec![255; 4]),
        JsValue::from(7.0),
        JsValue::from(4.0),
        JsValue::from(2.0),
        JsValue::from(1.0),
    ];
    assert!(paint(&arguments).as_bytes().is_some());
    for (index, invalid) in [
        (4, JsValue::Bytes(vec![255; 3])),
        (4, JsValue::from(255.0)),
        (5, JsValue::from(0.0)),
        (6, JsValue::from(1.0)),
        (7, JsValue::from(3.0)),
        (8, JsValue::from(2.0)),
        (7, JsValue::from(-1.0)),
        (8, JsValue::from(f64::NAN)),
    ] {
        let valid = std::mem::replace(&mut arguments[index], invalid);
        assert!(
            matches!(paint(&arguments), JsValue::Null),
            "argument {index}"
        );
        arguments[index] = valid;
        assert_eq!(arguments[2].as_bytes().unwrap(), destination);
    }
}

fn request() -> Request {
    Request {
        width: 4,
        height: 2,
        left: 0,
        top: 0,
        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        kind: Kind::Linear,
        geometry: vec![0.5, 0.0, 2.5, 0.0],
        stops: vec![
            Stop {
                offset: 0.0,
                channels: [255.0, 0.0, 0.0, 255.0],
            },
            Stop {
                offset: 1.0,
                channels: [0.0, 0.0, 255.0, 255.0],
            },
        ],
        opacity: 1.0,
    }
}

#[test]
fn upstream_shader_interpolates_straight_colors_before_premultiplication() {
    let mut request = request();
    request.stops[0].channels[3] = 0.0;
    let actual = render(&request, &[0; 32], None).unwrap();
    assert_eq!(&actual[..4], &[0, 0, 0, 0]);
    let midpoint = &actual[4..8];
    assert!((127..=129).contains(&midpoint[0]), "{midpoint:?}");
    assert!((127..=129).contains(&midpoint[2]), "{midpoint:?}");
    assert!((127..=128).contains(&midpoint[3]), "{midpoint:?}");
    assert_eq!(&actual[8..12], &[0, 0, 255, 255]);
}

#[test]
fn roi_and_paint_transform_sample_the_same_gradient_coordinates() {
    let request = request();
    let entire = render(&request, &[0; 32], None).unwrap();
    let mut region = request;
    region.left = 1;
    region.width = 2;
    let actual = render(&region, &[0; 16], None).unwrap();
    assert_eq!(&actual[..8], &entire[4..12]);
    region.left = 3;
    region.transform[4] = 2.0;
    assert_eq!(render(&region, &[0; 16], None).unwrap(), actual);
}

#[test]
fn coverage_and_global_opacity_are_applied_once_and_uncovered_pixels_are_preserved() {
    let mut request = request();
    request.opacity = 0.5;
    let mask = [0, 128, 255, 255, 0, 0, 0, 0];
    let destination = [7, 8, 9, 255].repeat(8);
    let actual = render(&request, &destination, Some(&mask)).unwrap();
    assert_eq!(&actual[..4], &destination[..4]);
    assert_eq!(&actual[16..], &destination[16..]);
    // Blue endpoint over an opaque destination with half opacity.
    assert_eq!(&actual[8..12], &[4, 4, 132, 255]);
    assert_eq!(destination, [7, 8, 9, 255].repeat(8));
}

#[test]
fn radial_two_circle_shader_and_conic_rotation_use_real_backend_geometry() {
    let mut request = request();
    request.kind = Kind::Radial;
    request.geometry = vec![0.5, 0.5, 0.0, 0.5, 0.5, 2.0];
    let actual = render(&request, &[0; 32], None).unwrap();
    assert_eq!(&actual[..4], &[255, 0, 0, 255]);
    assert_eq!(&actual[8..12], &[0, 0, 255, 255]);
    request.kind = Kind::Conic;
    request.geometry = vec![0.0, 0.5, 0.5];
    let initial = render(&request, &[0; 32], None).unwrap();
    assert_eq!(&initial[4..8], &[255, 0, 0, 255]);
    request.geometry[0] = std::f64::consts::PI;
    let rotated = render(&request, &[0; 32], None).unwrap();
    assert!((127..=128).contains(&rotated[4]));
    assert!((127..=128).contains(&rotated[6]));
}

#[test]
fn duplicate_stops_keep_order_and_single_radial_stop_does_not_fill_outside_cone() {
    let mut request = request();
    request.stops[0].offset = 0.5;
    request.stops[1].offset = 0.5;
    let actual = render(&request, &[0; 32], None).unwrap();
    assert_eq!(&actual[..4], &[255, 0, 0, 255]);
    assert_eq!(&actual[4..8], &[0, 0, 255, 255]);
    request.kind = Kind::Radial;
    request.geometry = vec![0.5, 0.5, 1.0, 2.5, 0.5, 1.0];
    request.stops.truncate(1);
    request.height = 4;
    let actual = render(&request, &[0; 64], None).unwrap();
    assert_eq!(
        actual[(3 * 4 + 1) * 4 + 3],
        0,
        "outside parallel-circle strip"
    );
}

#[test]
fn rejected_requests_return_fallback_not_partial_pixels() {
    let mut request = request();
    assert!(render(&request, &[0; 31], None).is_none());
    assert!(render(&request, &[0; 32], Some(&[0; 7])).is_none());
    request.opacity = f64::NAN;
    assert!(render(&request, &[0; 32], None).is_none());
    request.opacity = 1.0;
    request.geometry[0] = 1e100;
    assert!(render(&request, &[0; 32], None).is_none());
    request.geometry[0] = 0.5;
    request.stops[0].channels[0] = 256.0;
    assert!(render(&request, &[0; 32], None).is_none());
    request.stops[0].channels[0] = 255.0;
    request.transform = [0.0; 6];
    assert!(render(&request, &[0; 32], None).is_none());
    request.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    request.stops.swap(0, 1);
    assert!(render(&request, &[0; 32], None).is_none());
    request.stops = (0..=MAX_STOPS)
        .map(|index| Stop {
            offset: index as f64 / MAX_STOPS as f64,
            channels: [255.0; 4],
        })
        .collect();
    assert!(render(&request, &[0; 32], None).is_none());
    assert_eq!(paint(&[]), JsValue::Null);
}
