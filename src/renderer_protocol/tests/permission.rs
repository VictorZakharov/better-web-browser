use super::*;

#[test]
fn permission_frames_have_distinct_kinds_and_reject_the_wrong_direction() {
    let document = DocumentId::new(42).unwrap();
    let request = RendererMessage::PermissionRequest(PermissionRequest {
        document,
        request_id: 7,
        client: crate::fetch::RequestClient::default(),
        embedded: false,
        name: PermissionName::Geolocation,
    });
    let update = BrowserMessage::PermissionUpdate(PermissionUpdate {
        document,
        request_id: 7,
        name: PermissionName::Geolocation,
        state: PermissionState::Prompt,
        rejected: false,
    });
    let request_frame = encoded_renderer(&request);
    let update_frame = encoded_browser(&update);
    assert_eq!(&request_frame[8..10], &0x0220_u16.to_le_bytes());
    assert_eq!(&update_frame[8..10], &0x0221_u16.to_le_bytes());
    assert_eq!(
        FrameReader::new(Cursor::new(request_frame.clone()), session())
            .read_renderer()
            .unwrap(),
        request
    );
    assert_eq!(
        FrameReader::new(Cursor::new(update_frame.clone()), session())
            .read_browser()
            .unwrap(),
        update
    );
    assert!(matches!(
        FrameReader::new(Cursor::new(request_frame), session()).read_browser(),
        Err(ProtocolError::UnexpectedMessage(0x0220))
    ));
    assert!(matches!(
        FrameReader::new(Cursor::new(update_frame), session()).read_renderer(),
        Err(ProtocolError::UnexpectedMessage(0x0221))
    ));
}

#[test]
fn clipboard_permission_names_survive_both_protocol_directions() {
    let document = DocumentId::new(43).unwrap();
    for name in [
        PermissionName::ClipboardRead,
        PermissionName::ClipboardWrite,
    ] {
        let request = RendererMessage::PermissionRequest(PermissionRequest {
            document,
            request_id: 8,
            client: crate::fetch::RequestClient::default(),
            embedded: false,
            name,
        });
        let update = BrowserMessage::PermissionUpdate(PermissionUpdate {
            document,
            request_id: 8,
            name,
            state: PermissionState::Granted,
            rejected: false,
        });
        assert_eq!(
            FrameReader::new(Cursor::new(encoded_renderer(&request)), session())
                .read_renderer()
                .unwrap(),
            request
        );
        assert_eq!(
            FrameReader::new(Cursor::new(encoded_browser(&update)), session())
                .read_browser()
                .unwrap(),
            update
        );
    }
}
