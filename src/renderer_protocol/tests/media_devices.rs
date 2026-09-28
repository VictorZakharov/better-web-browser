use super::*;
use crate::fetch::RequestClient;

#[test]
fn media_device_frames_are_admitted_only_in_their_direction() {
    let document = DocumentId::new(7).unwrap();
    let request = RendererMessage::MediaDeviceRequest(MediaDeviceRequest {
        document,
        request_id: 9,
        client: RequestClient {
            id: 0,
            opaque: false,
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

    let update = BrowserMessage::MediaDeviceUpdate(MediaDeviceUpdate {
        document,
        request_id: 9,
        result: MediaDeviceResult::Presence {
            microphone: true,
            camera: false,
        },
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
