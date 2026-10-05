use super::*;

const SOURCE: [u8; 8] = [255, 0, 0, 255, 0, 0, 255, 255];

fn request() -> Request {
    Request {
        width: 4,
        height: 2,
        left: 0,
        top: 0,
        source_width: 2,
        source_height: 1,
        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        opacity: 1.0,
    }
}

#[test]
fn repeat_phase_affine_scale_and_roi_use_canvas_coordinates() {
    let mut request = request();
    let expected = SOURCE.repeat(4);
    assert_eq!(render(&request, &[0; 32], None, &SOURCE).unwrap(), expected);
    request.left = 1;
    let shifted = render(&request, &[0; 32], None, &SOURCE).unwrap();
    assert_eq!(&shifted[..8], &[0, 0, 255, 255, 255, 0, 0, 255]);
    request.transform[4] = 1.0;
    assert_eq!(render(&request, &[0; 32], None, &SOURCE).unwrap(), expected);
    request.left = 0;
    request.transform = [2.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let scaled = render(&request, &[0; 32], None, &SOURCE).unwrap();
    assert_eq!(
        &scaled[..16],
        &[
            255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255
        ]
    );
}

#[test]
fn mask_and_opacity_apply_once_without_mutating_owned_inputs() {
    let mut request = request();
    request.opacity = 0.5;
    let destination = [0, 255, 0, 255].repeat(8);
    let original = destination.clone();
    let mask = [0, 255, 128, 255, 255, 255, 255, 255];
    let output = render(&request, &destination, Some(&mask), &SOURCE).unwrap();
    assert_eq!(&output[..4], &destination[..4]);
    assert_eq!(&output[4..8], &[0, 128, 128, 255]);
    assert_eq!(&output[8..12], &[64, 191, 0, 255]);
    assert_eq!(destination, original);
    assert_eq!(SOURCE, [255, 0, 0, 255, 0, 0, 255, 255]);
}

#[test]
fn malformed_dimensions_buffers_and_transforms_fail_closed() {
    let mut request = request();
    assert!(render(&request, &[0; 31], None, &SOURCE).is_none());
    assert!(render(&request, &[0; 32], Some(&[0; 7]), &SOURCE).is_none());
    assert!(render(&request, &[0; 32], None, &SOURCE[..7]).is_none());
    request.transform[0] = 0.0;
    assert!(render(&request, &[0; 32], None, &SOURCE).is_none());
    request.transform[0] = f64::NAN;
    assert!(render(&request, &[0; 32], None, &SOURCE).is_none());
    request.transform[0] = 1.0;
    request.opacity = 2.0;
    assert!(render(&request, &[0; 32], None, &SOURCE).is_none());
    request.opacity = 1.0;
    request.source_width = 1_048_577;
    assert!(render(&request, &[0; 32], None, &SOURCE).is_none());
}

#[test]
fn closed_host_request_rejects_author_fields_and_wrong_argument_types() {
    let encoded = r#"{"width":4,"height":2,"left":0,"top":0,"source_width":2,"source_height":1,"transform":[1,0,0,1,0,0],"opacity":1}"#;
    let args = [
        JsValue::Null,
        JsValue::String(encoded.into()),
        JsValue::Bytes(vec![0; 32]),
        JsValue::Null,
        JsValue::Bytes(SOURCE.to_vec()),
    ];
    assert!(matches!(paint(&args), JsValue::Bytes(_)));
    let mut invalid = args;
    invalid[1] = JsValue::String(encoded.replace("\"opacity\":1", "\"opacity\":1,\"extra\":true"));
    assert!(matches!(paint(&invalid), JsValue::Null));
    invalid[1] = JsValue::String(encoded.into());
    invalid[3] = JsValue::String("mask".into());
    assert!(matches!(paint(&invalid), JsValue::Null));
}
