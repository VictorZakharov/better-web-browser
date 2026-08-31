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
                bgra: vec![0, 0, 0, 255],
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
            bgra: vec![0, 0, 0, 255],
        },
    });
    assert!(matches!(
        presentation.encode(),
        Err(ProtocolError::InvalidPayload("presented image count"))
    ));
}
