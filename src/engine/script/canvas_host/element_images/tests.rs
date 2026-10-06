use super::*;
use std::sync::Arc;

fn pixel(bgra: &[u8]) -> DecodedImage {
    DecodedImage {
        width: 1,
        height: 1,
        bgra: Arc::from(bgra),
    }
}

#[test]
fn snapshots_own_straight_rgba_and_share_only_immutable_decoder_storage() {
    let url = "https://example.test/a.png".to_owned();
    let image = pixel(&[16, 32, 64, 128]);
    let mut cache = ElementImages::default();
    cache.synchronize(
        &HashMap::from([(url.clone(), image.clone())]),
        &HashMap::from([(url.clone(), true)]),
    );
    assert!(Arc::ptr_eq(&cache.images[&url].0.bgra, &image.bgra));
    let JsValue::Array(mut first) = cache.snapshot(&url) else {
        panic!("snapshot")
    };
    let JsValue::Bytes(bytes) = &mut first[2] else {
        panic!("pixels")
    };
    assert_eq!(bytes, &[128, 64, 32, 128]);
    bytes.fill(255);
    let JsValue::Array(second) = cache.snapshot(&url) else {
        panic!("snapshot")
    };
    let JsValue::Bytes(bytes) = &second[2] else {
        panic!("pixels")
    };
    assert_eq!(bytes, &[128, 64, 32, 128]);
}

#[test]
fn missing_policy_opaque_response_and_layout_metadata_never_expose_pixels() {
    let url = "https://foreign.test/a.png".to_owned();
    let mut cache = ElementImages::default();
    let images = HashMap::from([(url.clone(), pixel(&[1, 2, 3, 255]))]);
    cache.synchronize(&images, &HashMap::new());
    assert!(matches!(cache.snapshot(&url), JsValue::Null));
    cache.synchronize(&images, &HashMap::from([(url.clone(), false)]));
    assert!(matches!(cache.snapshot(&url), JsValue::String(value) if value == "tainted"));
    cache.synchronize(
        &HashMap::from([(url.clone(), pixel(&[]))]),
        &HashMap::from([(url.clone(), true)]),
    );
    assert!(matches!(cache.snapshot(&url), JsValue::Null));
}

#[test]
fn zero_alpha_over_budget_and_invalid_stride_fail_closed() {
    let url = "https://example.test/a.png".to_owned();
    let origins = HashMap::from([(url.clone(), true)]);
    let mut cache = ElementImages::default();
    cache.synchronize(
        &HashMap::from([(url.clone(), pixel(&[255, 255, 255, 0]))]),
        &origins,
    );
    let JsValue::Array(snapshot) = cache.snapshot(&url) else {
        panic!("snapshot")
    };
    assert!(matches!(&snapshot[2], JsValue::Bytes(bytes) if bytes == &[0,0,0,0]));
    for image in [
        pixel(&[1, 2, 3]),
        DecodedImage {
            width: u32::MAX,
            height: u32::MAX,
            bgra: Arc::from([]),
        },
        DecodedImage {
            width: 0,
            height: 1,
            bgra: Arc::from([]),
        },
    ] {
        cache.synchronize(&HashMap::from([(url.clone(), image)]), &origins);
        assert!(matches!(cache.snapshot(&url), JsValue::Null));
    }
}
