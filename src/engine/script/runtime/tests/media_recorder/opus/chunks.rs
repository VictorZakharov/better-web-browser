use super::*;

#[test]
fn actual_stereo_blob_chunks_form_one_stream_with_exact_fractional_terminal_duration() {
    let (dom, mut runtime, request) = start("{mimeType: 'audio/ogg'}", "recorder.start(100);");
    for sequence in 1..=2 {
        capture(&mut runtime, request, sequence, 48_000, 2, 960);
    }
    run(&dom, &mut runtime, "recorder.requestData();");
    for sequence in 3..=5 {
        capture(&mut runtime, request, sequence, 48_000, 2, 960);
    }
    run_queued_events(&mut runtime);
    capture(&mut runtime, request, 6, 48_000, 2, 960);
    capture(&mut runtime, request, 7, 48_000, 2, 317);
    run(&dom, &mut runtime, "recorder.stop();");
    assert_eq!(attribute(&dom, "data-events"), "start,data,data,data,stop");
    assert_eq!(attribute(&dom, "data-parts"), "3");
    assert_eq!(attribute(&dom, "data-type"), "audio/ogg;codecs=opus");
    assert_eq!(attribute(&dom, "data-state"), "inactive");
    assert!(attribute(&dom, "data-timecodes").starts_with("0,"));
    let pcm = decoded(&dom, 2, 6_077);
    for channel in 0..2 {
        assert!(
            pcm.iter()
                .skip(channel)
                .step_by(2)
                .any(|sample| *sample > 0.1)
        );
        assert!(
            pcm.iter()
                .skip(channel)
                .step_by(2)
                .any(|sample| *sample < -0.1)
        );
    }
    assert!(
        pcm.chunks_exact(2)
            .any(|pair| (pair[0] - pair[1]).abs() > 0.1)
    );
}

#[test]
fn each_native_opus_capture_rate_preserves_short_stop_frame_count_at_48khz() {
    for rate in [8_000, 12_000, 16_000, 24_000, 48_000] {
        let (dom, mut runtime, request) =
            start("{mimeType: 'audio/ogg;codecs=opus'}", "recorder.start();");
        capture(&mut runtime, request, 1, rate, 1, rate / 50);
        capture(&mut runtime, request, 2, rate, 1, 17);
        run(&dom, &mut runtime, "recorder.stop();");
        assert_eq!(attribute(&dom, "data-events"), "start,data,stop");
        let expected = u64::from(rate / 50 + 17) * 48_000 / u64::from(rate);
        let pcm = decoded(&dom, 1, expected);
        assert!(pcm.iter().any(|sample| sample.abs() > 0.01), "rate {rate}");
    }
}

#[test]
fn paused_packets_are_omitted_and_disabled_track_records_real_silence() {
    let (dom, mut runtime, request) = start(
        "{mimeType: 'audio/ogg'}",
        "recorder.start(); recorder.pause();",
    );
    capture(&mut runtime, request, 1, 48_000, 1, 960);
    run(
        &dom,
        &mut runtime,
        "track.enabled = false; recorder.resume();",
    );
    for sequence in 2..=6 {
        capture(&mut runtime, request, sequence, 48_000, 1, 960);
    }
    run(&dom, &mut runtime, "recorder.stop();");
    assert_eq!(
        attribute(&dom, "data-events"),
        "start,pause,resume,data,stop"
    );
    let pcm = decoded(&dom, 1, 4_800);
    assert!(pcm.iter().all(|sample| sample.abs() < 0.0001));
}

#[test]
fn dropped_capture_packet_is_timed_silence_not_a_shorter_recording() {
    let (dom, mut runtime, request) = start("{mimeType: 'audio/ogg'}", "recorder.start();");
    run(&dom, &mut runtime, "track.enabled = false;");
    capture(&mut runtime, request, 1, 48_000, 1, 960);
    run(
        &dom,
        &mut runtime,
        "recorder.requestData(); track.enabled = true;",
    );
    // Sequence 2 was dropped by the bounded capture queue; the timestamp retains
    // its 20 ms duration without asking the codec for concealment or fake PCM.
    capture(&mut runtime, request, 3, 48_000, 1, 960);
    run(&dom, &mut runtime, "recorder.stop();");
    assert_eq!(attribute(&dom, "data-events"), "start,data,data,stop");
    assert_eq!(attribute(&dom, "data-timecodes"), "0,20");
    let pcm = decoded(&dom, 1, 2_880);
    assert!(pcm[..1_400].iter().all(|sample| sample.abs() < 0.0001));
    assert!(pcm[1_920..].iter().any(|sample| sample.abs() > 0.01));
}
