use super::*;
use std::io::Cursor;

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
            limits: RendererLimits::default(),
        },
        BrowserMessage::Ping(42),
        BrowserMessage::Shutdown,
        BrowserMessage::ProtocolFailure("bad frame".into()),
        BrowserMessage::Test(TestCommand::Crash),
        BrowserMessage::Test(TestCommand::AccessViolation),
        BrowserMessage::Test(TestCommand::OutOfMemory),
        BrowserMessage::Test(TestCommand::StackOverflow),
        BrowserMessage::Test(TestCommand::Hang),
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
        RendererMessage::TimeAdvanced {
            document,
            next_timer_micros: Some(7_500),
        },
        RendererMessage::TimeAdvanced {
            document,
            next_timer_micros: None,
        },
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
    version[4..6].copy_from_slice(&2_u16.to_le_bytes());
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
            containment: ContainmentReport {
                app_container: true,
                no_console_window: true,
                minimal_environment: true,
            },
        })
        .unwrap();
    ready[HEADER_LENGTH + 32] = 2;
    assert!(matches!(
        FrameReader::new(Cursor::new(ready), session()).read_renderer(),
        Err(ProtocolError::InvalidPayload("boolean"))
    ));

    let mut advanced = encoded_renderer(&RendererMessage::TimeAdvanced {
        document: DocumentId::new(1).unwrap(),
        next_timer_micros: Some(10),
    });
    advanced[HEADER_LENGTH + 8] = 2;
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
        limits: RendererLimits::default(),
    });
    bytes[HEADER_LENGTH + 36..HEADER_LENGTH + 40]
        .copy_from_slice(&((MAX_FRAME_PAYLOAD + 1) as u32).to_le_bytes());

    assert!(matches!(
        FrameReader::new(Cursor::new(bytes), session()).read_browser(),
        Err(ProtocolError::InvalidPayload("renderer limits"))
    ));
}
