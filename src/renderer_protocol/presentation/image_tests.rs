use super::tests::sample;
use super::*;

#[test]
fn aggregate_presented_image_limit_round_trips_and_fails_closed() {
    let mut presentation = sample();
    presentation.images = (0..crate::limits::MAX_PRESENTED_IMAGES)
        .map(|index| PresentedImage {
            url: format!("https://example.test/{index}.png"),
            image: DecodedImage {
                width: 1,
                height: 1,
                bgra: vec![0, 0, 0, 255].into(),
            },
        })
        .collect();
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    assert_eq!(decoded.images.len(), crate::limits::MAX_PRESENTED_IMAGES);

    presentation.images.push(PresentedImage {
        url: "https://example.test/overflow.png".into(),
        image: DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![0, 0, 0, 255].into(),
        },
    });
    assert!(matches!(
        presentation.encode(),
        Err(ProtocolError::InvalidPayload("presented image count"))
    ));
}

#[test]
fn retired_image_keys_round_trip_and_cannot_alias_live_updates() {
    let mut presentation = sample();
    presentation.retired_image_keys = vec!["breeze-internal:canvas:1".into()];
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    assert_eq!(decoded.retired_image_keys, presentation.retired_image_keys);

    presentation
        .retired_image_keys
        .push("breeze-internal:canvas:1".into());
    assert!(matches!(
        presentation.encode(),
        Err(ProtocolError::InvalidPayload("retired image key"))
    ));
    presentation.retired_image_keys.pop();
    presentation.images.push(PresentedImage {
        url: "breeze-internal:canvas:1".into(),
        image: DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![0; 4].into(),
        },
    });
    assert!(matches!(
        presentation.encode(),
        Err(ProtocolError::InvalidPayload("retired image key"))
    ));
}
