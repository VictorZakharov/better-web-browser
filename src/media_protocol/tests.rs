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
        flac_hresult: -1,
        h264_decoders: 1,
        aac_decoders: 0,
        flac_decoders: 0,
        probe_micros: 1,
    };
    assert!(impossible_report.validate(MediaLimits::default()).is_err());
    let impossible_flac_report = MediaCapabilityReport {
        startup_hresult: 0,
        h264_hresult: 0,
        aac_hresult: 0,
        flac_hresult: -1,
        h264_decoders: 0,
        aac_decoders: 0,
        flac_decoders: 1,
        probe_micros: 1,
    };
    assert!(
        impossible_flac_report
            .validate(MediaLimits::default())
            .is_err()
    );

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
fn audio_only_report_requires_real_audio_and_no_video_metadata() {
    let report = MediaDecodeReport {
        buffered: MediaBufferedExtent {
            audio_end_100ns: 10_000_000,
            ..Default::default()
        },
        encoded_bytes: 88_244,
        video_codec: MediaCodecFamily::None,
        audio_codec: MediaCodecFamily::Pcm,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 0,
        video_height: 0,
        audio_sample_rate: 44_100,
        audio_channels: 1,
        video_samples: 0,
        audio_samples: 10,
        video_decoded_bytes: 0,
        audio_decoded_bytes: 88_200,
        video_first_timestamp_100ns: 0,
        video_last_timestamp_100ns: 0,
        audio_first_timestamp_100ns: 0,
        audio_last_timestamp_100ns: 9_000_000,
        duration_100ns: 10_000_000,
        decode_micros: 100,
    };
    assert!(report.validate(MediaLimits::default()).is_ok());
    assert_eq!(MediaCodecFamily::Flac.wire_code(), 6);
    assert_eq!(
        MediaCodecFamily::from_wire(6).unwrap(),
        MediaCodecFamily::Flac
    );
    assert!(
        MediaDecodeReport {
            audio_codec: MediaCodecFamily::Flac,
            ..report
        }
        .validate(MediaLimits::default())
        .is_ok()
    );
    for forged in [
        MediaDecodeReport {
            video_width: 1,
            ..report
        },
        MediaDecodeReport {
            video_samples: 1,
            ..report
        },
        MediaDecodeReport {
            buffered: MediaBufferedExtent {
                video_end_100ns: 10_000_000,
                ..report.buffered
            },
            ..report
        },
        MediaDecodeReport {
            audio_codec: MediaCodecFamily::None,
            ..report
        },
        MediaDecodeReport {
            audio_samples: 0,
            ..report
        },
    ] {
        assert!(forged.validate(MediaLimits::default()).is_err());
    }
    let mut bytes = Vec::new();
    MediaFrameWriter::new(&mut bytes, session(15))
        .send_worker(&WorkerMediaMessage::Decoded {
            request_id: 8,
            report,
            frame: None,
        })
        .unwrap();
    assert_eq!(
        MediaFrameReader::new(Cursor::new(bytes), session(15))
            .read_worker()
            .unwrap(),
        WorkerMediaMessage::Decoded {
            request_id: 8,
            report,
            frame: None,
        }
    );
}

#[test]
fn video_only_report_requires_real_video_and_no_audio_metadata() {
    let report = MediaDecodeReport {
        buffered: MediaBufferedExtent {
            video_end_100ns: 10_000_000,
            ..Default::default()
        },
        encoded_bytes: 12_345,
        video_codec: MediaCodecFamily::H264,
        audio_codec: MediaCodecFamily::None,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 320,
        video_height: 240,
        audio_sample_rate: 0,
        audio_channels: 0,
        video_samples: 25,
        audio_samples: 0,
        video_decoded_bytes: 25 * 320 * 240 * 3 / 2,
        audio_decoded_bytes: 0,
        video_first_timestamp_100ns: 0,
        video_last_timestamp_100ns: 9_600_000,
        audio_first_timestamp_100ns: 0,
        audio_last_timestamp_100ns: 0,
        duration_100ns: 10_000_000,
        decode_micros: 100,
    };
    assert!(report.validate(MediaLimits::default()).is_ok());
    for forged in [
        MediaDecodeReport {
            audio_sample_rate: 44_100,
            ..report
        },
        MediaDecodeReport {
            audio_samples: 1,
            ..report
        },
        MediaDecodeReport {
            audio_decoded_bytes: 1,
            ..report
        },
        MediaDecodeReport {
            audio_last_timestamp_100ns: 1,
            ..report
        },
        MediaDecodeReport {
            buffered: MediaBufferedExtent {
                audio_end_100ns: 10_000_000,
                ..report.buffered
            },
            ..report
        },
        MediaDecodeReport {
            video_samples: 0,
            ..report
        },
        MediaDecodeReport {
            video_codec: MediaCodecFamily::None,
            ..report
        },
    ] {
        assert!(forged.validate(MediaLimits::default()).is_err());
    }
    let mut bytes = Vec::new();
    let frame = MediaVideoFrameMetadata {
        source_id: 4,
        frame_id: 6,
        timestamp_100ns: 0,
        duration_100ns: 400_000,
        width: 320,
        height: 240,
        stride: 320,
        format: MediaPixelFormat::Nv12,
        data_length: 115_200,
    };
    MediaFrameWriter::new(&mut bytes, session(16))
        .send_worker(&WorkerMediaMessage::Decoded {
            request_id: 9,
            report,
            frame: Some(frame),
        })
        .unwrap();
    assert_eq!(
        MediaFrameReader::new(Cursor::new(bytes), session(16))
            .read_worker()
            .unwrap(),
        WorkerMediaMessage::Decoded {
            request_id: 9,
            report,
            frame: Some(frame),
        }
    );
}

#[test]
fn nonce_debug_output_never_discloses_secret_bytes() {
    let nonce = Nonce::new([0xab; 32]);
    let debug = format!("{nonce:?}");
    assert!(!debug.contains("ab"));
    assert!(debug.contains("redacted"));
}
