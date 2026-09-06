use super::*;
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
            frame_id: 6,
            encoded_length: 13_932,
        })
        .unwrap();
    browser_writer
        .send_browser(&BrowserMediaMessage::DecodeTracks {
            request_id: 13,
            video_source_id: 5,
            audio_source_id: 6,
            frame_id: 7,
            video_length: 12_000,
            audio_length: 1_932,
        })
        .unwrap();
    browser_writer
        .send_browser(&BrowserMediaMessage::AppendTracks {
            request_id: 14,
            source_id: 5,
            video_source_id: 7,
            audio_source_id: 8,
            video_length: 8_000,
            audio_length: 1_000,
        })
        .unwrap();
    browser_writer
        .send_browser(&BrowserMediaMessage::AcknowledgeFrame {
            source_id: 4,
            frame_id: 6,
        })
        .unwrap();
    browser_writer
        .send_browser(&BrowserMediaMessage::RequestFrame {
            source_id: 4,
            frame_id: 7,
        })
        .unwrap();
    browser_writer
        .send_browser(&BrowserMediaMessage::SetPlayback {
            source_id: 4,
            playing: true,
            volume_millis: 625,
        })
        .unwrap();
    browser_writer
        .send_browser(&BrowserMediaMessage::PlaybackState { source_id: 4 })
        .unwrap();
    browser_writer
        .send_browser(&BrowserMediaMessage::SeekPlayback {
            source_id: 4,
            position_100ns: 5_000_000,
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
            frame_id: 6,
            encoded_length: 13_932,
        }
    );
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::DecodeTracks {
            request_id: 13,
            video_source_id: 5,
            audio_source_id: 6,
            frame_id: 7,
            video_length: 12_000,
            audio_length: 1_932,
        }
    );
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::AppendTracks {
            request_id: 14,
            source_id: 5,
            video_source_id: 7,
            audio_source_id: 8,
            video_length: 8_000,
            audio_length: 1_000,
        }
    );
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::AcknowledgeFrame {
            source_id: 4,
            frame_id: 6,
        }
    );
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::RequestFrame {
            source_id: 4,
            frame_id: 7,
        }
    );
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::SetPlayback {
            source_id: 4,
            playing: true,
            volume_millis: 625,
        }
    );
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::PlaybackState { source_id: 4 }
    );
    assert_eq!(
        browser_reader.read_browser().unwrap(),
        BrowserMediaMessage::SeekPlayback {
            source_id: 4,
            position_100ns: 5_000_000,
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

    let state = MediaPlaybackState {
        source_id: 4,
        position_100ns: 2_500_000,
        duration_100ns: 10_000_000,
        playing: true,
        ended: false,
    };
    let mut worker_bytes = Vec::new();
    MediaFrameWriter::new(&mut worker_bytes, session(9))
        .send_worker(&WorkerMediaMessage::PlaybackState(state))
        .unwrap();
    assert_eq!(
        MediaFrameReader::new(Cursor::new(worker_bytes), session(9))
            .read_worker()
            .unwrap(),
        WorkerMediaMessage::PlaybackState(state)
    );

    let mut worker_bytes = Vec::new();
    MediaFrameWriter::new(&mut worker_bytes, session(9))
        .send_worker(&WorkerMediaMessage::DecodeFailed {
            request_id: 13,
            error: "create video source reader: unsupported byte stream".into(),
        })
        .unwrap();
    assert_eq!(
        MediaFrameReader::new(Cursor::new(worker_bytes), session(9))
            .read_worker()
            .unwrap(),
        WorkerMediaMessage::DecodeFailed {
            request_id: 13,
            error: "create video source reader: unsupported byte stream".into(),
        }
    );

    let decoded = MediaDecodeReport {
        buffered: MediaBufferedExtent {
            video_end_100ns: 10_292_000,
            audio_end_100ns: 10_292_000,
            ..Default::default()
        },
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
    let frame = MediaVideoFrameMetadata {
        source_id: 4,
        frame_id: 6,
        timestamp_100ns: 0,
        duration_100ns: 333_333,
        width: 320,
        height: 240,
        stride: 320,
        format: MediaPixelFormat::Nv12,
        data_length: 115_200,
    };
    {
        let mut worker_writer = MediaFrameWriter::new(&mut worker_bytes, session(9));
        worker_writer
            .send_worker(&WorkerMediaMessage::Decoded {
                request_id: 12,
                report: decoded,
                frame,
            })
            .unwrap();
        worker_writer
            .send_worker(&WorkerMediaMessage::Appended {
                buffered: decoded.buffered,
                request_id: 14,
                source_id: 4,
                encoded_bytes: 22_932,
                duration_100ns: 20_000_000,
            })
            .unwrap();
        worker_writer
            .send_worker(&WorkerMediaMessage::FrameAcknowledged {
                source_id: 4,
                frame_id: 6,
            })
            .unwrap();
        worker_writer
            .send_worker(&WorkerMediaMessage::FrameReady {
                frame: MediaVideoFrameMetadata {
                    frame_id: 7,
                    timestamp_100ns: 333_333,
                    ..frame
                },
            })
            .unwrap();
        worker_writer
            .send_worker(&WorkerMediaMessage::EndOfStream { source_id: 4 })
            .unwrap();
    }
    let mut worker_reader = MediaFrameReader::new(Cursor::new(worker_bytes), session(9));
    assert_eq!(
        worker_reader.read_worker().unwrap(),
        WorkerMediaMessage::Decoded {
            request_id: 12,
            report: decoded,
            frame,
        }
    );
    assert_eq!(
        worker_reader.read_worker().unwrap(),
        WorkerMediaMessage::Appended {
            buffered: decoded.buffered,
            request_id: 14,
            source_id: 4,
            encoded_bytes: 22_932,
            duration_100ns: 20_000_000,
        }
    );
    assert_eq!(
        worker_reader.read_worker().unwrap(),
        WorkerMediaMessage::FrameAcknowledged {
            source_id: 4,
            frame_id: 6,
        }
    );
    assert_eq!(
        worker_reader.read_worker().unwrap(),
        WorkerMediaMessage::FrameReady {
            frame: MediaVideoFrameMetadata {
                frame_id: 7,
                timestamp_100ns: 333_333,
                ..frame
            },
        }
    );
    assert_eq!(
        worker_reader.read_worker().unwrap(),
        WorkerMediaMessage::EndOfStream { source_id: 4 }
    );
}
