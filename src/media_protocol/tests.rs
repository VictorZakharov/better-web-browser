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
    browser_writer
        .send_browser(&BrowserMediaMessage::DecodeSource {
            request_id: 12,
            source_id: 4,
            encoded_length: 13_932,
        })
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
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::DecodeSource {
            request_id: 12,
            source_id: 4,
            encoded_length: 13_932,
        }
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

    let decoded = MediaDecodeReport {
        encoded_bytes: 13_932,
        video_codec: MediaCodecFamily::H264,
        audio_codec: MediaCodecFamily::AacLc,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 320,
        video_height: 240,
        audio_sample_rate: 44_100,
        audio_channels: 2,
        video_samples: 31,
        audio_samples: 44,
        video_decoded_bytes: 3_571_200,
        audio_decoded_bytes: 176_400,
        video_first_timestamp_100ns: 0,
        video_last_timestamp_100ns: 10_000_000,
        audio_first_timestamp_100ns: 0,
        audio_last_timestamp_100ns: 10_000_000,
        duration_100ns: 10_292_000,
        decode_micros: 2_500,
    };
    let mut worker_bytes = Vec::new();
    MediaFrameWriter::new(&mut worker_bytes, session(9))
        .send_worker(&WorkerMediaMessage::Decoded {
            request_id: 12,
            report: decoded,
        })
        .unwrap();
    assert_eq!(
        MediaFrameReader::new(Cursor::new(worker_bytes), session(9))
            .read_worker()
            .unwrap(),
        WorkerMediaMessage::Decoded {
            request_id: 12,
            report: decoded,
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

    let impossible_decode = MediaDecodeReport {
        encoded_bytes: 1,
        video_codec: MediaCodecFamily::H264,
        audio_codec: MediaCodecFamily::AacLc,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 0,
        video_height: 240,
        audio_sample_rate: 44_100,
        audio_channels: 2,
        video_samples: 1,
        audio_samples: 1,
        video_decoded_bytes: 1,
        audio_decoded_bytes: 1,
        video_first_timestamp_100ns: 0,
        video_last_timestamp_100ns: 0,
        audio_first_timestamp_100ns: 0,
        audio_last_timestamp_100ns: 0,
        duration_100ns: 1,
        decode_micros: 1,
    };
    assert!(impossible_decode.validate(MediaLimits::default()).is_err());
}

#[test]
fn nonce_debug_output_never_discloses_secret_bytes() {
    let nonce = Nonce::new([0xab; 32]);
    let debug = format!("{nonce:?}");
    assert!(!debug.contains("ab"));
    assert!(debug.contains("redacted"));
}
