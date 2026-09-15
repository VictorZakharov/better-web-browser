use super::*;

#[test]
fn fetch_credit_is_directional_and_rejects_zero_request_identity() {
    let message = RendererMessage::FetchResponseConsumed {
        document: DocumentId::new(4).unwrap(),
        request_id: 7,
        total: 65536,
    };
    assert!(matches!(
        FrameReader::new(Cursor::new(encoded_renderer(&message)), session()).read_browser(),
        Err(ProtocolError::UnexpectedMessage(0x010c))
    ));
    let mut bytes = encoded_renderer(&message);
    bytes[HEADER_LENGTH + 8..HEADER_LENGTH + 16].fill(0);
    assert!(
        FrameReader::new(Cursor::new(bytes), session())
            .read_renderer()
            .is_err()
    );
}
#[test]
fn state_and_stream_messages_round_trip() {
    let document = DocumentId::new(11).unwrap();
    let browser = vec![
        BrowserMessage::CookieSnapshot(CookieStateSnapshot {
            document,
            version: 3,
            header: "theme=dark".into(),
        }),
        BrowserMessage::StorageSnapshotStart(StorageSnapshotStart {
            document,
            area: StorageAreaKind::Local,
            version: 4,
            entry_count: 1,
        }),
        BrowserMessage::StorageSnapshotEntry(StorageSnapshotEntry {
            document,
            area: StorageAreaKind::Local,
            entry: StorageEntry {
                key: "theme".into(),
                value: "dark".into(),
            },
        }),
        BrowserMessage::StorageSnapshotEnd(StorageSnapshotEnd {
            document,
            area: StorageAreaKind::Local,
            version: 4,
        }),
        BrowserMessage::FetchResponseStart(FetchResponseHead {
            request_id: 9,
            result: FetchResponseResult::Success {
                response_type: FetchResponseType::Cors,
                urls: vec!["https://example.test/data".into()],
                status: 200,
                headers: vec![("content-type".into(), "text/plain".into())],
            },
        }),
        BrowserMessage::FetchResponseChunk(TransferChunk {
            transfer_id: 9,
            offset: 0,
            bytes: b"first".to_vec(),
        }),
        BrowserMessage::FetchResponseEnd(FetchResponseEnd {
            request_id: 9,
            total_length: 5,
        }),
        BrowserMessage::FetchResponseAbort(FetchResponseAbort {
            request_id: 10,
            error: BrowserFetchError {
                kind: BrowserFetchErrorKind::Aborted,
                message: "navigation replaced".into(),
            },
        }),
    ];
    let mut bytes = Vec::new();
    let mut writer = FrameWriter::new(&mut bytes, session());
    for message in &browser {
        writer.send_browser(message).unwrap();
    }
    let mut reader = FrameReader::new(Cursor::new(bytes), session());
    for expected in browser {
        assert_eq!(reader.read_browser().unwrap(), expected);
    }

    let renderer = vec![
        RendererMessage::FetchResponseConsumed {
            document,
            request_id: 1,
            total: 65536,
        },
        RendererMessage::FetchRequestAbort {
            document,
            request_id: 9,
        },
        RendererMessage::CookieMutation(CookieMutation {
            document,
            assignment: "theme=light; Path=/".into(),
        }),
        RendererMessage::StorageMutation(StorageMutationRequest {
            sequence: 1,
            source_url: "https://example.com/".into(),
            document,
            mutation: StorageMutation {
                area: StorageAreaKind::Session,
                expected_version: 8,
                operation: StorageOperation::Set {
                    key: "draft".into(),
                    value: "saved".into(),
                },
            },
        }),
        RendererMessage::StateSnapshotApplied(StateSnapshotApplied {
            document,
            kind: StateSnapshotKind::LocalStorage,
            version: 8,
        }),
    ];
    let mut bytes = Vec::new();
    let mut writer = FrameWriter::new(&mut bytes, session());
    for message in &renderer {
        writer.send_renderer(message).unwrap();
    }
    let mut reader = FrameReader::new(Cursor::new(bytes), session());
    for expected in renderer {
        assert_eq!(reader.read_renderer().unwrap(), expected);
    }
}

#[test]
fn state_messages_reject_oversized_values_before_writing() {
    let document = DocumentId::new(1).unwrap();
    let mut writer = FrameWriter::new(Vec::new(), session());
    let cookie = RendererMessage::CookieMutation(CookieMutation {
        document,
        assignment: "x".repeat(4_097),
    });
    assert!(matches!(
        writer.send_renderer(&cookie),
        Err(ProtocolError::InvalidPayload("cookie mutation"))
    ));

    let item = BrowserMessage::StorageSnapshotEntry(StorageSnapshotEntry {
        document,
        area: StorageAreaKind::Local,
        entry: StorageEntry {
            key: "x".repeat(crate::limits::MAX_STORAGE_KEY_BYTES + 1).into(),
            value: "".into(),
        },
    });
    assert!(matches!(
        writer.send_browser(&item),
        Err(ProtocolError::InvalidPayload("storage snapshot entry"))
    ));
}
