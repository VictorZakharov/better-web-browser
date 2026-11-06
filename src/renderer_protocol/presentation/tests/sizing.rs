//! A presentation's encoded length, measured without building the payload.
use super::*;

#[test]
fn encoded_length_matches_the_payload_and_its_errors() {
    let mut presentation = sample();
    presentation.images.push(PresentedImage {
        url: "https://example.test/image.png".into(),
        image: DecodedImage {
            width: 2,
            height: 1,
            bgra: vec![0; 8].into(),
        },
    });
    assert_eq!(
        presentation.encoded_len().unwrap(),
        presentation.encode().unwrap().len()
    );
    presentation.runtime.viewport_wheel_delta_y = f32::NAN;
    assert_eq!(
        format!("{:?}", presentation.encoded_len().unwrap_err()),
        format!("{:?}", presentation.encode().unwrap_err())
    );
}
