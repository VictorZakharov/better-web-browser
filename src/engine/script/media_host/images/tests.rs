use super::*;

fn node(index: u128) -> NodeId {
    NodeId::from_wire((1_u128 << 64) | index).unwrap()
}
fn image(width: u32, height: u32, pixels: Vec<u8>) -> DecodedImage {
    DecodedImage {
        width,
        height,
        bgra: pixels.into(),
    }
}

#[test]
fn snapshots_convert_bgra_without_mutating_or_exposing_decoder_storage() {
    let mut frames = MediaImages::default();
    let original = image(2, 1, vec![10, 20, 30, 255, 100, 110, 120, 128]);
    let retained = original.bgra.clone();
    frames.replace(node(1), original, true).unwrap();
    for _ in 0..2 {
        let JsValue::Array(snapshot) = frames.snapshot(node(1)) else {
            panic!("missing snapshot")
        };
        assert_eq!(snapshot[0].as_number(), Some(2.0));
        assert_eq!(snapshot[1].as_number(), Some(1.0));
        assert_eq!(
            snapshot[2].as_bytes().unwrap(),
            &[30, 20, 10, 255, 120, 110, 100, 128]
        );
    }
    assert_eq!(&*retained, &[10, 20, 30, 255, 100, 110, 120, 128]);
    assert!(matches!(frames.snapshot(node(2)), JsValue::Null));
}

#[test]
fn origin_policy_is_bound_to_each_current_frame_not_the_element_attributes() {
    let mut frames = MediaImages::default();
    frames
        .replace(node(1), image(1, 1, vec![1, 2, 3, 255]), false)
        .unwrap();
    assert_eq!(frames.snapshot(node(1)).string_value(), "tainted");
    frames
        .replace(node(2), image(1, 1, vec![4, 5, 6, 255]), true)
        .unwrap();
    assert!(matches!(frames.snapshot(node(2)), JsValue::Array(_)));
    assert_eq!(frames.snapshot(node(1)).string_value(), "tainted");
    frames
        .replace(node(1), image(1, 1, vec![7, 8, 9, 255]), true)
        .unwrap();
    assert!(matches!(frames.snapshot(node(1)), JsValue::Array(_)));
    frames.remove(node(1));
    assert!(matches!(frames.snapshot(node(1)), JsValue::Null));
    assert_eq!(frames.bytes, 4);
    frames.remove(node(1));
    assert_eq!(frames.bytes, 4);
}

#[test]
fn admission_is_transactional_and_cannot_overflow_or_retain_malformed_pixels() {
    let mut frames = MediaImages::default();
    frames
        .replace(node(1), image(1, 1, vec![1, 2, 3, 255]), true)
        .unwrap();
    for malformed in [
        image(0, 1, vec![]),
        image(1, 1, vec![1, 2, 3]),
        image(u32::MAX, u32::MAX, vec![]),
        image(4096, 4096, vec![]),
    ] {
        assert!(frames.replace(node(1), malformed, false).is_err());
        assert_eq!(frames.bytes, 4);
        assert!(matches!(frames.snapshot(node(1)), JsValue::Array(_)));
    }
    for index in 2..=8 {
        frames
            .replace(node(index), image(1, 1, vec![0; 4]), true)
            .unwrap();
    }
    assert!(
        frames
            .replace(node(9), image(1, 1, vec![0; 4]), true)
            .is_err()
    );
    frames
        .replace(node(1), image(2, 1, vec![0; 8]), false)
        .unwrap();
    assert_eq!(frames.bytes, 36);
    frames.remove(node(3));
    frames
        .replace(node(9), image(1, 1, vec![0; 4]), true)
        .unwrap();
    assert_eq!(frames.frames.len(), 8);
}

#[test]
fn replacing_large_frames_reuses_budget_and_releases_old_arcs() {
    let mut frames = MediaImages::default();
    let full = image(2048, 2048, vec![0; 16 * 1024 * 1024]);
    let observer = std::sync::Arc::downgrade(&full.bgra);
    frames.replace(node(1), full.clone(), true).unwrap();
    frames.replace(node(2), full.clone(), true).unwrap();
    assert!(
        frames
            .replace(node(3), image(1, 1, vec![0; 4]), true)
            .is_err()
    );
    frames
        .replace(node(1), image(1, 1, vec![0; 4]), true)
        .unwrap();
    frames.remove(node(2));
    assert_eq!(frames.bytes, 4);
    drop(full);
    assert!(observer.upgrade().is_none());
}

#[test]
fn disabling_capture_blacks_current_pixels_without_changing_extent_or_ownership() {
    let mut frames = MediaImages::default();
    let original = image(2, 1, vec![3, 2, 1, 255, 6, 5, 4, 255]);
    let retained = original.bgra.clone();
    frames.replace(node(1), original, true).unwrap();
    frames.blacken(node(1));
    let JsValue::Array(snapshot) = frames.snapshot(node(1)) else {
        panic!("missing disabled frame")
    };
    assert_eq!(snapshot[0].as_number(), Some(2.0));
    assert_eq!(
        snapshot[2].as_bytes().unwrap(),
        &[0, 0, 0, 255, 0, 0, 0, 255]
    );
    assert_eq!(&*retained, &[3, 2, 1, 255, 6, 5, 4, 255]);
    assert_eq!(frames.bytes, 8);
    frames.blacken(node(2));
    assert_eq!(frames.frames.len(), 1);
}
