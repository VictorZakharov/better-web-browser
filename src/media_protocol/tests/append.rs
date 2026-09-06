use super::*;

#[test]
fn independent_track_append_round_trips_but_empty_batches_fail_closed() {
    for (video_length, audio_length) in [(64, 0), (0, 64), (64, 64)] {
        let command = BrowserMediaMessage::AppendTracks {
            request_id: 1,
            source_id: 1,
            video_source_id: 3,
            audio_source_id: 4,
            video_length,
            audio_length,
        };
        let mut bytes = Vec::new();
        MediaFrameWriter::new(&mut bytes, session(1))
            .send_browser(&command)
            .unwrap();
        assert_eq!(
            MediaFrameReader::new(Cursor::new(bytes), session(1))
                .read_browser()
                .unwrap(),
            command
        );
    }
    let empty = BrowserMediaMessage::AppendTracks {
        request_id: 1,
        source_id: 1,
        video_source_id: 3,
        audio_source_id: 4,
        video_length: 0,
        audio_length: 0,
    };
    assert!(
        MediaFrameWriter::new(Vec::new(), session(1))
            .send_browser(&empty)
            .is_err()
    );
}

#[test]
fn appended_extent_preserves_the_absent_track_and_rejects_impossible_ranges() {
    let buffered = MediaBufferedExtent {
        video_start_100ns: 10,
        video_end_100ns: 20,
        ..Default::default()
    };
    let report = WorkerMediaMessage::Appended {
        request_id: 1,
        source_id: 1,
        encoded_bytes: 64,
        duration_100ns: 20,
        buffered,
    };
    let mut bytes = Vec::new();
    MediaFrameWriter::new(&mut bytes, session(1))
        .send_worker(&report)
        .unwrap();
    assert_eq!(
        MediaFrameReader::new(Cursor::new(bytes), session(1))
            .read_worker()
            .unwrap(),
        report
    );
    for invalid in [
        MediaBufferedExtent::default(),
        MediaBufferedExtent {
            video_start_100ns: -1,
            ..buffered
        },
        MediaBufferedExtent {
            video_start_100ns: 20,
            ..buffered
        },
        MediaBufferedExtent {
            audio_start_100ns: 1,
            ..buffered
        },
        MediaBufferedExtent {
            video_end_100ns: u64::MAX,
            ..buffered
        },
    ] {
        assert!(invalid.validate().is_err());
    }
    let inconsistent = WorkerMediaMessage::Appended {
        request_id: 1,
        source_id: 1,
        encoded_bytes: 64,
        duration_100ns: 19,
        buffered,
    };
    assert!(
        MediaFrameWriter::new(Vec::new(), session(1))
            .send_worker(&inconsistent)
            .is_err()
    );
}
