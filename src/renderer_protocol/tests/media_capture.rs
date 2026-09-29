use super::*;
use crate::fetch::RequestClient;

#[test]
fn capture_control_and_sample_frames_are_directional() {
    let document = DocumentId::new(7).unwrap();
    let request = RendererMessage::MediaCaptureRequest(MediaCaptureRequest {
        document,
        request_id: 9,
        client: RequestClient {
            id: 1,
            opaque: false,
        },
        action: MediaCaptureAction::Start {
            camera: true,
            microphone: false,
        },
    });
    let bytes = encoded_renderer(&request);
    assert_eq!(
        FrameReader::new(Cursor::new(bytes.clone()), session())
            .read_renderer()
            .unwrap(),
        request
    );
    assert!(
        FrameReader::new(Cursor::new(bytes), session())
            .read_browser()
            .is_err()
    );

    let update = BrowserMessage::MediaCaptureUpdate(MediaCaptureUpdate {
        document,
        request_id: 9,
        event: MediaCaptureEvent::Started {
            camera: true,
            microphone: false,
        },
    });
    let bytes = encoded_browser(&update);
    assert_eq!(
        FrameReader::new(Cursor::new(bytes.clone()), session())
            .read_browser()
            .unwrap(),
        update
    );
    assert!(
        FrameReader::new(Cursor::new(bytes), session())
            .read_renderer()
            .is_err()
    );

    let sample = BrowserMessage::MediaCaptureFrame(MediaCaptureFrame {
        document,
        request_id: 9,
        track_id: 1,
        sequence: 1,
        timestamp_100ns: 10,
        kind: MediaCaptureFrameKind::VideoNv12,
        width_or_rate: 2,
        height_or_frames: 2,
        stride_or_channels: 2,
        bytes: vec![0; 6],
    });
    let bytes = encoded_browser(&sample);
    assert_eq!(
        FrameReader::new(Cursor::new(bytes.clone()), session())
            .read_browser()
            .unwrap(),
        sample
    );
    assert!(
        FrameReader::new(Cursor::new(bytes), session())
            .read_renderer()
            .is_err()
    );
}

#[test]
fn capture_rejects_no_tracks_and_oversized_samples() {
    let document = DocumentId::new(1).unwrap();
    let request = MediaCaptureRequest {
        document,
        request_id: 1,
        client: RequestClient {
            id: 0,
            opaque: false,
        },
        action: MediaCaptureAction::Start {
            camera: false,
            microphone: false,
        },
    };
    assert!(request.validate().is_err());

    let sample = MediaCaptureFrame {
        document,
        request_id: 1,
        track_id: 1,
        sequence: 1,
        timestamp_100ns: 0,
        kind: MediaCaptureFrameKind::VideoNv12,
        width_or_rate: 1280,
        height_or_frames: 720,
        stride_or_channels: 1280,
        bytes: vec![0; MAX_CAPTURE_FRAME_BYTES + 1],
    };
    assert!(sample.validate().is_err());
}
