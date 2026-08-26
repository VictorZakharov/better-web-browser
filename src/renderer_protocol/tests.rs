use super::*;
use crate::storage::{StorageAreaKind, StorageEntry, StorageMutation, StorageOperation};
use std::io::Cursor;

mod input;

fn session() -> RendererSessionId {
    RendererSessionId::new(7).unwrap()
}

fn encoded_browser(message: &BrowserMessage) -> Vec<u8> {
    let mut writer = FrameWriter::new(Vec::new(), session());
    writer.send_browser(message).unwrap();
    writer.into_inner()
}

fn encoded_renderer(message: &RendererMessage) -> Vec<u8> {
    let mut writer = FrameWriter::new(Vec::new(), session());
    writer.send_renderer(message).unwrap();
    writer.into_inner()
}

#[test]
fn browser_messages_round_trip() {
    let messages = [
        BrowserMessage::Hello {
            nonce: Nonce::new([0x5a; 32]),
            context: BrowsingContextId::new(9).unwrap(),
            limits: RendererLimits::default(),
        },
        BrowserMessage::Ping(42),
        BrowserMessage::Shutdown,
        BrowserMessage::ProtocolFailure("bad frame".into()),
        BrowserMessage::Test(TestCommand::InternalError),
        BrowserMessage::Test(TestCommand::DocumentError),
        BrowserMessage::Test(TestCommand::Crash),
        BrowserMessage::Test(TestCommand::AccessViolation),
        BrowserMessage::Test(TestCommand::OutOfMemory),
        BrowserMessage::Test(TestCommand::StackOverflow),
        BrowserMessage::Test(TestCommand::Hang),
        BrowserMessage::Test(TestCommand::DelayCommandRead { millis: 250 }),
        BrowserMessage::Test(TestCommand::Padding { bytes: 32 }),
        BrowserMessage::Test(TestCommand::WriteMalformedFrame),
        BrowserMessage::Test(TestCommand::ProbeRestrictions {
            loopback_port: 8080,
        }),
    ];
    let mut bytes = Vec::new();
    let mut writer = FrameWriter::new(&mut bytes, session());
    for message in &messages {
        writer.send_browser(message).unwrap();
    }
    let mut reader = FrameReader::new(Cursor::new(bytes), session());
    for expected in messages {
        assert_eq!(reader.read_browser().unwrap(), expected);
    }
}

#[test]
fn renderer_messages_round_trip() {
    let report = RestrictionReport {
        child_launch_denied: true,
        loopback_denied: true,
        internet_denied: true,
        child_error: 5,
        loopback_error: 10_013,
        internet_error: 10_013,
    };
    let messages = [
        RendererMessage::Ready {
            nonce: Nonce::new([0xa5; 32]),
            context: BrowsingContextId::new(9).unwrap(),
            containment: ContainmentReport {
                app_container: true,
                no_console_window: true,
                minimal_environment: true,
            },
        },
        RendererMessage::Pong(9),
        RendererMessage::Diagnostic(RendererDiagnostic::new(1, "ready").unwrap()),
        RendererMessage::Restrictions(report),
        RendererMessage::ShutdownComplete,
    ];
    let mut bytes = Vec::new();
    let mut writer = FrameWriter::new(&mut bytes, session());
    for message in &messages {
        writer.send_renderer(message).unwrap();
    }
    let mut reader = FrameReader::new(Cursor::new(bytes), session());
    for expected in messages {
        assert_eq!(reader.read_renderer().unwrap(), expected);
    }
}

#[test]
fn pointer_cursor_results_round_trip_and_reject_invalid_fields() {
    let document = DocumentId::new(11).unwrap();
    for cursor in [PointerCursor::Default, PointerCursor::Pointer] {
        let message = RendererMessage::PointerCursor(PointerCursorResult {
            document,
            sequence: 27,
            cursor,
        });
        let decoded = FrameReader::new(Cursor::new(encoded_renderer(&message)), session())
            .read_renderer()
            .unwrap();
        assert_eq!(decoded, message);
    }

    let valid = RendererMessage::PointerCursor(PointerCursorResult {
        document,
        sequence: 27,
        cursor: PointerCursor::Pointer,
    });
    let mut zero_sequence = encoded_renderer(&valid);
    zero_sequence[HEADER_LENGTH + 8..HEADER_LENGTH + 16].fill(0);
    assert!(matches!(
        FrameReader::new(Cursor::new(zero_sequence), session()).read_renderer(),
        Err(ProtocolError::InvalidPayload("pointer cursor sequence"))
    ));

    let mut invalid_cursor = encoded_renderer(&valid);
    invalid_cursor[HEADER_LENGTH + 16] = 3;
    assert!(matches!(
        FrameReader::new(Cursor::new(invalid_cursor), session()).read_renderer(),
        Err(ProtocolError::InvalidPayload("pointer cursor"))
    ));
}

#[test]
fn document_start_diagnostic_selectors_round_trip_and_are_bounded() {
    let start = DocumentStart {
        document: DocumentId::new(11).unwrap(),
        url: "https://example.test/".into(),
        status: 200,
        content_type: "text/html".into(),
        diagnostic_selectors: vec!["#main".into(), ".content".into()],
        body_length: 10,
        viewport: PresentedViewport {
            width: 800.0,
            height: 600.0,
            style_width: 800.0,
            dpi: 96,
        },
    };
    let message = BrowserMessage::BeginDocument(start.clone());
    let decoded = FrameReader::new(Cursor::new(encoded_browser(&message)), session())
        .read_browser()
        .unwrap();
    assert_eq!(decoded, message);

    let mut oversized = start;
    oversized.diagnostic_selectors =
        vec!["*".into(); crate::limits::MAX_PAGE_DIAGNOSTIC_SELECTORS + 1];
    let mut writer = FrameWriter::new(Vec::new(), session());
    assert!(matches!(
        writer.send_browser(&BrowserMessage::BeginDocument(oversized)),
        Err(ProtocolError::InvalidPayload(
            "document diagnostic selectors"
        ))
    ));
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
        RendererMessage::CookieMutation(CookieMutation {
            document,
            assignment: "theme=light; Path=/".into(),
        }),
        RendererMessage::StorageMutation(StorageMutationRequest {
            document,
            mutation: StorageMutation {
                area: StorageAreaKind::Session,
                expected_version: 7,
                operation: StorageOperation::Set {
                    key: "draft".into(),
                    value: "saved".into(),
                },
            },
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
            key: "x".repeat(crate::limits::MAX_STORAGE_KEY_BYTES + 1),
            value: String::new(),
        },
    });
    assert!(matches!(
        writer.send_browser(&item),
        Err(ProtocolError::InvalidPayload("storage snapshot entry"))
    ));
}

#[test]
fn document_clock_messages_round_trip() {
    let document = DocumentId::new(11).unwrap();
    let browser = BrowserMessage::AdvanceTime {
        document,
        elapsed_micros: 12_345,
        max_callbacks: 1,
    };
    let mut reader = FrameReader::new(Cursor::new(encoded_browser(&browser)), session());
    assert_eq!(reader.read_browser().unwrap(), browser);

    for expected in [
        RendererMessage::RuntimeUpdate(RendererRuntimeUpdate {
            document,
            runtime: RuntimeReport {
                scripts_executed: 2,
                console: vec!["clock advanced".into()],
                runtime_active: true,
                ..RuntimeReport::default()
            },
            load: PageLoadReport {
                script_micros: 91,
                ..PageLoadReport::default()
            },
            next_timer_micros: Some(7_500),
        }),
        RendererMessage::RuntimeUpdate(RendererRuntimeUpdate {
            document,
            runtime: RuntimeReport::default(),
            load: PageLoadReport::default(),
            next_timer_micros: None,
        }),
    ] {
        let mut reader = FrameReader::new(Cursor::new(encoded_renderer(&expected)), session());
        assert_eq!(reader.read_renderer().unwrap(), expected);
    }
}

#[test]
fn nonce_hex_round_trips_without_debug_disclosure() {
    let nonce = Nonce::new([0xab; 32]);
    assert_eq!(Nonce::from_hex(&nonce.to_hex()).unwrap(), nonce);
    assert_eq!(format!("{nonce:?}"), "Nonce([redacted])");
}

#[test]
fn rejects_oversized_payload_before_allocation() {
    let mut bytes = encoded_browser(&BrowserMessage::Shutdown);
    bytes[12..16].copy_from_slice(&((MAX_CONTROL_PAYLOAD + 1) as u32).to_le_bytes());
    let error = FrameReader::new(Cursor::new(bytes), session())
        .read_browser()
        .unwrap_err();
    assert!(matches!(error, ProtocolError::PayloadTooLarge(_)));
}

#[test]
fn rejects_stale_session_before_reading_payload() {
    let mut bytes = encoded_browser(&BrowserMessage::Ping(1));
    bytes[16..24].copy_from_slice(&99_u64.to_le_bytes());
    let error = FrameReader::new(Cursor::new(bytes), session())
        .read_browser()
        .unwrap_err();
    assert!(matches!(error, ProtocolError::WrongSession { .. }));
}

#[test]
fn rejects_non_monotonic_sequence() {
    let mut bytes = encoded_browser(&BrowserMessage::Ping(1));
    bytes[24..32].copy_from_slice(&2_u64.to_le_bytes());
    let error = FrameReader::new(Cursor::new(bytes), session())
        .read_browser()
        .unwrap_err();
    assert!(matches!(error, ProtocolError::WrongSequence { .. }));
}

#[test]
fn rejects_incompatible_version_and_reserved_flags() {
    let mut version = encoded_browser(&BrowserMessage::Shutdown);
    version[4..6].copy_from_slice(&(PROTOCOL_MAJOR + 1).to_le_bytes());
    assert!(matches!(
        FrameReader::new(Cursor::new(version), session()).read_browser(),
        Err(ProtocolError::IncompatibleVersion { .. })
    ));

    let mut flags = encoded_browser(&BrowserMessage::Shutdown);
    flags[10..12].copy_from_slice(&1_u16.to_le_bytes());
    assert!(matches!(
        FrameReader::new(Cursor::new(flags), session()).read_browser(),
        Err(ProtocolError::ReservedFlags(1))
    ));
}

#[test]
fn rejects_messages_from_the_wrong_direction() {
    let bytes = encoded_browser(&BrowserMessage::Ping(1));
    let error = FrameReader::new(Cursor::new(bytes), session())
        .read_renderer()
        .unwrap_err();
    assert!(matches!(error, ProtocolError::UnexpectedMessage(3)));
}

#[test]
fn rejects_invalid_boolean_and_utf8_payloads() {
    let mut ready = Vec::new();
    let mut writer = FrameWriter::new(&mut ready, session());
    writer
        .send_renderer(&RendererMessage::Ready {
            nonce: Nonce::new([1; 32]),
            context: BrowsingContextId::new(1).unwrap(),
            containment: ContainmentReport {
                app_container: true,
                no_console_window: true,
                minimal_environment: true,
            },
        })
        .unwrap();
    ready[HEADER_LENGTH + 40] = 2;
    assert!(matches!(
        FrameReader::new(Cursor::new(ready), session()).read_renderer(),
        Err(ProtocolError::InvalidPayload("boolean"))
    ));

    let mut advanced = encoded_renderer(&RendererMessage::RuntimeUpdate(RendererRuntimeUpdate {
        document: DocumentId::new(1).unwrap(),
        runtime: RuntimeReport::default(),
        load: PageLoadReport::default(),
        next_timer_micros: Some(10),
    }));
    let timer_presence = advanced.len() - 9;
    advanced[timer_presence] = 2;
    assert!(matches!(
        FrameReader::new(Cursor::new(advanced), session()).read_renderer(),
        Err(ProtocolError::InvalidPayload("wire boolean"))
    ));

    let mut text = encoded_browser(&BrowserMessage::ProtocolFailure("ok".into()));
    text[HEADER_LENGTH] = 0xff;
    assert!(matches!(
        FrameReader::new(Cursor::new(text), session()).read_browser(),
        Err(ProtocolError::InvalidUtf8)
    ));
}

#[test]
fn rejects_limits_above_the_protocol_contract() {
    let mut bytes = encoded_browser(&BrowserMessage::Hello {
        nonce: Nonce::new([1; 32]),
        context: BrowsingContextId::new(1).unwrap(),
        limits: RendererLimits::default(),
    });
    bytes[HEADER_LENGTH + 44..HEADER_LENGTH + 48]
        .copy_from_slice(&((MAX_FRAME_PAYLOAD + 1) as u32).to_le_bytes());

    assert!(matches!(
        FrameReader::new(Cursor::new(bytes), session()).read_browser(),
        Err(ProtocolError::InvalidPayload("renderer limits"))
    ));
}
