use super::*;
use crate::fetch::RequestClient;

#[test]
fn geolocation_frames_are_admitted_only_in_their_direction() {
    let document = DocumentId::new(7).unwrap();
    let request = RendererMessage::GeolocationRequest(GeolocationRequest {
        document,
        request_id: 9,
        client: RequestClient {
            id: 0,
            opaque: false,
        },
        action: GeolocationAction::Start {
            watch: true,
            high_accuracy: false,
            timeout_millis: 2500,
            maximum_age_millis: 0,
        },
    });
    let frame = encoded_renderer(&request);
    assert_eq!(
        FrameReader::new(Cursor::new(frame.clone()), session())
            .read_renderer()
            .unwrap(),
        request
    );
    assert!(
        FrameReader::new(Cursor::new(frame), session())
            .read_browser()
            .is_err()
    );

    let update = BrowserMessage::GeolocationUpdate(GeolocationUpdate {
        document,
        request_id: 9,
        terminal: false,
        event: GeolocationEvent::Position(GeolocationPosition {
            latitude: 43.65,
            longitude: -79.38,
            accuracy: 10.0,
            altitude: None,
            altitude_accuracy: None,
            heading: None,
            speed: None,
            timestamp_millis: 1234,
        }),
    });
    let frame = encoded_browser(&update);
    assert_eq!(
        FrameReader::new(Cursor::new(frame.clone()), session())
            .read_browser()
            .unwrap(),
        update
    );
    assert!(
        FrameReader::new(Cursor::new(frame), session())
            .read_renderer()
            .is_err()
    );
}
