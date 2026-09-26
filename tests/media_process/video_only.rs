use super::*;

#[test]
fn contained_worker_decodes_and_clocks_complete_h264_without_audio() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // This is the WPT H.264 track remux documented in fixtures/media/README.md.
    let fixture = decode_base64(include_str!(
        "../fixtures/media/test-1s-video-fragmented.mp4.base64"
    ));
    assert_eq!(fixture.len(), 12_345);
    let mut session =
        MediaSession::launch(decode_options()).expect("launch contained media worker");
    let decoded = session
        .decode_owned_fixture_frame(&fixture)
        .expect("decode H.264-only MP4 in contained worker");
    let report = decoded.report;
    let source_id = decoded.frame.metadata.source_id;
    assert_eq!(report.video_codec, MediaCodecFamily::H264);
    assert_eq!(report.audio_codec, MediaCodecFamily::None);
    assert_eq!((report.audio_sample_rate, report.audio_channels), (0, 0));
    assert_eq!((report.audio_samples, report.audio_decoded_bytes), (0, 0));
    assert_eq!(report.buffered.audio_end_100ns, 0);
    assert!(report.video_samples > 0);
    assert!(report.buffered.video_end_100ns > 0);
    assert_eq!(decoded.frame.metadata.format, MediaPixelFormat::Nv12);

    let playing = session
        .set_owned_fixture_playback(source_id, true, 1_000)
        .expect("play H.264-only media");
    assert!(playing.playing);
    std::thread::sleep(Duration::from_millis(25));
    let paused = session
        .set_owned_fixture_playback(source_id, false, 1_000)
        .expect("pause H.264-only media");
    assert!(!paused.playing);
    assert!(paused.position_100ns > playing.position_100ns);
    let still_paused = session
        .owned_fixture_playback_state(source_id)
        .expect("query paused H.264-only clock");
    assert_eq!(still_paused.position_100ns, paused.position_100ns);

    let ending = session
        .seek_owned_fixture_playback(source_id, report.duration_100ns.saturating_sub(100_000))
        .unwrap_or_else(|error| panic!("seek H.264-only media: {error}; {:?}", session.snapshot()));
    assert!(ending.position_100ns < report.duration_100ns);
    let playing = session
        .set_owned_fixture_playback(source_id, true, 1_000)
        .expect("resume H.264-only media");
    assert!(playing.playing);
    std::thread::sleep(Duration::from_millis(25));
    let ended = session
        .owned_fixture_playback_state(source_id)
        .expect("query ended H.264-only clock");
    assert_eq!(ended.position_100ns, report.duration_100ns);
    assert!(ended.ended);
    assert!(!ended.playing);
    let rewind = session
        .seek_owned_fixture_playback(source_id, 0)
        .expect("rewind H.264-only media");
    assert_eq!(rewind.position_100ns, 0);
    assert!(!rewind.ended);

    let sequence = session
        .decode_owned_fixture_frames(&fixture, report.video_samples as usize)
        .expect("pull every H.264-only frame through the contained worker");
    assert_eq!(sequence.frames.len(), report.video_samples as usize);
    assert!(
        sequence
            .frames
            .windows(2)
            .all(|pair| pair[0].metadata.timestamp_100ns <= pair[1].metadata.timestamp_100ns)
    );
    session.shutdown().expect("stop contained media worker");
}

#[test]
fn contained_worker_seeks_nonfragmented_h264_without_audio() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // The stream-copy, ordinary MP4 remux is documented in fixtures/media/README.md.
    let fixture = decode_base64(include_str!("../fixtures/media/test-1s-video.mp4.base64"));
    assert_eq!(fixture.len(), 12_366);
    let mut session =
        MediaSession::launch(decode_options()).expect("launch contained media worker");
    let decoded = session
        .decode_owned_fixture_frame(&fixture)
        .expect("decode nonfragmented H.264-only MP4 in contained worker");
    let report = decoded.report;
    let source_id = decoded.frame.metadata.source_id;
    assert_eq!(report.video_codec, MediaCodecFamily::H264);
    assert_eq!(report.audio_codec, MediaCodecFamily::None);
    assert_eq!(report.audio_samples, 0);
    assert!(report.video_samples > 0);
    assert_eq!(
        (decoded.frame.metadata.width, decoded.frame.metadata.height),
        (320, 240)
    );

    let seeked = session
        .seek_owned_fixture_playback(source_id, report.duration_100ns / 2)
        .expect("seek nonfragmented H.264-only MP4");
    assert!(seeked.position_100ns > 0);
    assert!(seeked.position_100ns < report.duration_100ns);
    let playing = session
        .set_owned_fixture_playback(source_id, true, 1_000)
        .expect("play nonfragmented H.264-only MP4 after seek");
    assert!(playing.playing);
    let paused = session
        .set_owned_fixture_playback(source_id, false, 1_000)
        .expect("pause nonfragmented H.264-only MP4");
    assert!(!paused.playing);
    session.shutdown().expect("stop contained media worker");
}
