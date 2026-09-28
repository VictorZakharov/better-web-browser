use super::*;

#[test]
fn broadcast_command_and_delivery_round_trip_on_full_wire() {
    let document = DocumentId::new(11).unwrap();
    for operation in [
        BroadcastOperation::Open {
            name: "d8000061".into(),
        },
        BroadcastOperation::Post {
            serialized: r#"{"t":"object","id":1,"v":[]}"#.into(),
        },
        BroadcastOperation::Close,
    ] {
        let message = RendererMessage::BroadcastCommand(BroadcastCommand {
            document,
            channel_id: 9,
            operation,
        });
        assert_eq!(
            &encoded_renderer(&message)[8..10],
            &0x0200_u16.to_le_bytes()
        );
        assert_eq!(
            FrameReader::new(Cursor::new(encoded_renderer(&message)), session())
                .read_renderer()
                .unwrap(),
            message
        );
        assert!(
            FrameReader::new(Cursor::new(encoded_renderer(&message)), session())
                .read_browser()
                .is_err()
        );
    }
    let message = BrowserMessage::BroadcastDelivery(BroadcastDelivery {
        document,
        channel_id: 8,
        origin: "https://example.test".into(),
        serialized: "42".into(),
    });
    assert_eq!(&encoded_browser(&message)[8..10], &0x0201_u16.to_le_bytes());
    assert_eq!(
        FrameReader::new(Cursor::new(encoded_browser(&message)), session())
            .read_browser()
            .unwrap(),
        message
    );
    assert!(
        FrameReader::new(Cursor::new(encoded_browser(&message)), session())
            .read_renderer()
            .is_err()
    );
}

#[test]
fn broadcast_frames_reject_invalid_names_ids_and_unbounded_payloads() {
    let document = DocumentId::new(11).unwrap();
    let command = |channel_id, operation| {
        RendererMessage::BroadcastCommand(BroadcastCommand {
            document,
            channel_id,
            operation,
        })
    };
    for message in [
        command(0, BroadcastOperation::Close),
        command(1, BroadcastOperation::Open { name: "xyz".into() }),
        command(
            1,
            BroadcastOperation::Open {
                name: "0".repeat(4097),
            },
        ),
        command(
            1,
            BroadcastOperation::Post {
                serialized: "x".repeat(131073),
            },
        ),
    ] {
        assert!(
            FrameWriter::new(Vec::new(), session())
                .send_renderer(&message)
                .is_err()
        );
    }
    let valid = command(1, BroadcastOperation::Close);
    let mut invalid_operation = encoded_renderer(&valid);
    invalid_operation[HEADER_LENGTH + 16] = 9;
    assert!(matches!(
        FrameReader::new(Cursor::new(invalid_operation), session()).read_renderer(),
        Err(ProtocolError::InvalidPayload("broadcast operation"))
    ));
    let mut invalid_id = encoded_renderer(&valid);
    invalid_id[HEADER_LENGTH + 8..HEADER_LENGTH + 16].fill(0);
    assert!(matches!(
        FrameReader::new(Cursor::new(invalid_id), session()).read_renderer(),
        Err(ProtocolError::InvalidPayload(
            "broadcast channel identifier"
        ))
    ));
}
