use super::*;
mod append;
mod roundtrip;
use std::io::Cursor;

fn session(value: u64) -> MediaSessionId {
    MediaSessionId::new(value).unwrap()
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
        buffered: MediaBufferedExtent {
            video_end_100ns: 1,
            audio_end_100ns: 1,
            ..Default::default()
        },
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

    let streamed_decode = MediaDecodeReport {
        video_width: 1280,
        video_height: 720,
        video_samples: 600,
        audio_samples: 480,
        video_decoded_bytes: 1280 * 720 * 3 / 2 * 600,
        audio_decoded_bytes: 480 * 4096,
        duration_100ns: 10 * 10_000_000,
        ..impossible_decode
    };
    assert!(
        streamed_decode.validate(MediaLimits::default()).is_ok(),
        "cumulative pull-decoded output must not be treated as resident memory"
    );
    assert!(
        MediaDecodeReport {
            video_decoded_bytes: u64::MAX,
            ..streamed_decode
        }
        .validate(MediaLimits::default())
        .is_err()
    );
}

#[test]
fn nonce_debug_output_never_discloses_secret_bytes() {
    let nonce = Nonce::new([0xab; 32]);
    let debug = format!("{nonce:?}");
    assert!(!debug.contains("ab"));
    assert!(debug.contains("redacted"));
}
