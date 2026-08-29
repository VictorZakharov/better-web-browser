use super::*;
use std::io::Cursor;

fn session(value: u64) -> MediaSessionId {
    MediaSessionId::new(value).unwrap()
}

#[test]
fn browser_and_worker_messages_round_trip() {
    let nonce = Nonce::new([7; 32]);
    let mut browser_bytes = Vec::new();
    let mut browser_writer = MediaFrameWriter::new(&mut browser_bytes, session(9));
    browser_writer
        .send_browser(&BrowserMediaMessage::Hello {
            nonce,
            limits: MediaLimits::default(),
        })
        .unwrap();
    browser_writer
        .send_browser(&BrowserMediaMessage::Probe { request_id: 11 })
        .unwrap();
    let mut browser_reader = MediaFrameReader::new(Cursor::new(browser_bytes), session(9));
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::Hello {
            nonce,
            limits: MediaLimits::default()
        }
    );
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::Probe { request_id: 11 }
    );

    let report = MediaCapabilityReport {
        startup_hresult: 0,
        h264_hresult: 0,
        aac_hresult: 0,
        h264_decoders: 2,
        aac_decoders: 1,
        probe_micros: 120,
    };
    let mut worker_bytes = Vec::new();
    MediaFrameWriter::new(&mut worker_bytes, session(9))
        .send_worker(&WorkerMediaMessage::Capability {
            request_id: 11,
            report,
        })
        .unwrap();
    assert_eq!(
        MediaFrameReader::new(Cursor::new(worker_bytes), session(9))
            .read_worker()
            .unwrap(),
        WorkerMediaMessage::Capability {
            request_id: 11,
            report
        }
    );
}

#[test]
fn media_protocol_rejects_wrong_direction_and_stale_session() {
    let mut bytes = Vec::new();
    MediaFrameWriter::new(&mut bytes, session(3))
        .send_browser(&BrowserMediaMessage::Ping(1))
        .unwrap();
    assert!(matches!(
        MediaFrameReader::new(Cursor::new(bytes.clone()), session(3)).read_worker(),
        Err(MediaProtocolError::UnexpectedMessage(3))
    ));
    assert!(matches!(
        MediaFrameReader::new(Cursor::new(bytes), session(4)).read_browser(),
        Err(MediaProtocolError::WrongSession {
            expected: 4,
            actual: 3
        })
    ));
}

#[test]
fn media_protocol_rejects_oversized_payload_before_allocation() {
    let mut bytes = Vec::new();
    MediaFrameWriter::new(&mut bytes, session(5))
        .send_browser(&BrowserMediaMessage::Ping(1))
        .unwrap();
    bytes[12..16]
        .copy_from_slice(&((crate::limits::MAX_MEDIA_CONTROL_PAYLOAD + 1) as u32).to_le_bytes());
    assert!(matches!(
        MediaFrameReader::new(Cursor::new(bytes), session(5)).read_browser(),
        Err(MediaProtocolError::PayloadTooLarge(_))
    ));
}

#[test]
fn media_limits_and_capabilities_fail_closed() {
    let invalid_limits = MediaLimits {
        max_tracks: 0,
        ..MediaLimits::default()
    };
    assert!(invalid_limits.validate().is_err());

    let impossible_report = MediaCapabilityReport {
        startup_hresult: -1,
        h264_hresult: -1,
        aac_hresult: -1,
        h264_decoders: 1,
        aac_decoders: 0,
        probe_micros: 1,
    };
    assert!(impossible_report.validate(MediaLimits::default()).is_err());
}

#[test]
fn nonce_debug_output_never_discloses_secret_bytes() {
    let nonce = Nonce::new([0xab; 32]);
    let debug = format!("{nonce:?}");
    assert!(!debug.contains("ab"));
    assert!(debug.contains("redacted"));
}
