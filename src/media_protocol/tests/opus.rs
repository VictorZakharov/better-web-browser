use super::*;

fn opus_report() -> MediaDecodeReport {
    MediaDecodeReport {
        buffered: MediaBufferedExtent {
            audio_end_100ns: 4_000_000,
            ..Default::default()
        },
        encoded_bytes: 1910,
        video_codec: MediaCodecFamily::None,
        audio_codec: MediaCodecFamily::Opus,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 0,
        video_height: 0,
        audio_sample_rate: 48_000,
        audio_channels: 1,
        video_samples: 0,
        audio_samples: 21,
        video_decoded_bytes: 0,
        audio_decoded_bytes: 38_400,
        video_first_timestamp_100ns: 0,
        video_last_timestamp_100ns: 0,
        audio_first_timestamp_100ns: 0,
        audio_last_timestamp_100ns: 3_930_000,
        duration_100ns: 4_000_000,
        decode_micros: 1,
    }
}

#[test]
fn opus_reports_round_trip_without_fabricated_video() {
    assert_eq!(MediaCodecFamily::Opus.wire_code(), 8);
    assert_eq!(
        MediaCodecFamily::from_wire(8).unwrap(),
        MediaCodecFamily::Opus
    );
    assert!(MediaCodecFamily::from_wire(9).is_err());
    for channels in [1, 2] {
        let report = MediaDecodeReport {
            audio_channels: channels,
            ..opus_report()
        };
        report.validate(MediaLimits::default()).unwrap();
        let message = WorkerMediaMessage::Decoded {
            request_id: 1,
            report,
            frame: None,
        };
        let mut bytes = Vec::new();
        MediaFrameWriter::new(&mut bytes, session(6))
            .send_worker(&message)
            .unwrap();
        let parsed = MediaFrameReader::new(Cursor::new(bytes), session(6))
            .read_worker()
            .unwrap();
        assert_eq!(parsed, message);
    }
}

#[test]
fn opus_reports_reject_unsupported_pcm_shapes_and_video_combinations() {
    let report = opus_report();
    for rate in [0, 8_000, 44_100, 96_000, 384_000] {
        assert!(
            MediaDecodeReport {
                audio_sample_rate: rate,
                ..report
            }
            .validate(MediaLimits::default())
            .is_err()
        );
    }
    for channels in [0, 3, 8, 32] {
        assert!(
            MediaDecodeReport {
                audio_channels: channels,
                ..report
            }
            .validate(MediaLimits::default())
            .is_err()
        );
    }
    assert!(
        MediaDecodeReport {
            video_codec: MediaCodecFamily::Opus,
            ..report
        }
        .validate(MediaLimits::default())
        .is_err()
    );
    assert!(
        MediaDecodeReport {
            video_codec: MediaCodecFamily::H264,
            ..report
        }
        .validate(MediaLimits::default())
        .is_err()
    );
    assert!(
        MediaDecodeReport {
            video_width: 1,
            ..report
        }
        .validate(MediaLimits::default())
        .is_err()
    );
}
