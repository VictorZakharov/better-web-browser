use super::*;

#[test]
fn unsupported_44100hz_capture_errors_without_relabeling_and_same_recorder_recovers() {
    let (dom, mut runtime, request) = start("{mimeType: 'audio/ogg'}", "recorder.start();");
    capture(&mut runtime, request, 1, 44_100, 1, 882);
    run_queued_events(&mut runtime);
    assert_eq!(
        attribute(&dom, "data-events"),
        "start,NotSupportedError,data,stop"
    );
    assert_eq!(attribute(&dom, "data-state"), "inactive");
    assert!(
        bytes(&dom).is_empty(),
        "unsupported PCM must not acquire an Opus header"
    );
    run(&dom, &mut runtime, "recorder.start();");
    capture(&mut runtime, request, 2, 48_000, 1, 960);
    run(&dom, &mut runtime, "recorder.stop();");
    assert_eq!(
        attribute(&dom, "data-events"),
        "start,NotSupportedError,data,stop,start,data,stop"
    );
    assert!(
        decoded(&dom, 1, 960)
            .iter()
            .any(|sample| sample.abs() > 0.01)
    );
}

#[test]
fn changing_recorded_track_set_errors_after_preserving_gathered_opus() {
    let (dom, mut runtime, request) = start("{mimeType: 'audio/ogg'}", "recorder.start();");
    capture(&mut runtime, request, 1, 48_000, 1, 960);
    run(&dom, &mut runtime, "stream.removeTrack(track);");
    assert_eq!(
        attribute(&dom, "data-events"),
        "start,InvalidModificationError,data,stop"
    );
    assert_eq!(attribute(&dom, "data-state"), "inactive");
    assert!(
        decoded(&dom, 1, 960)
            .iter()
            .any(|sample| sample.abs() > 0.01)
    );
}

#[test]
fn ended_microphone_finishes_short_opus_recording_in_data_then_stop_order() {
    let (dom, mut runtime, request) = start("{mimeType: 'audio/ogg'}", "recorder.start();");
    capture(&mut runtime, request, 1, 48_000, 1, 17);
    let ended = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: request,
        event: MediaCaptureEvent::TrackEnded { track_id: 2 },
    });
    assert!(ended.errors.is_empty(), "{:?}", ended.errors);
    run_queued_events(&mut runtime);
    assert_eq!(attribute(&dom, "data-events"), "start,data,stop");
    assert_eq!(attribute(&dom, "data-state"), "inactive");
    decoded(&dom, 1, 17);
}

#[test]
fn immediate_opus_stop_and_restart_retires_slots_before_queued_events() {
    let (dom, mut runtime, _) = start(
        "{mimeType: 'audio/ogg'}",
        "for (let index = 0; index < 9; index++) { recorder.start(); recorder.stop(); }",
    );
    // The runtime may yield its bounded task turn before all 27 recorder events
    // finish. Pump advertised immediate work, not a guessed number of turns.
    for _ in 0..32 {
        run_queued_events(&mut runtime);
        if runtime.next_timer_delay() != Some(Duration::ZERO) {
            break;
        }
    }
    assert_ne!(runtime.next_timer_delay(), Some(Duration::ZERO));
    assert_eq!(attribute(&dom, "data-state"), "inactive");
    let expected = std::iter::repeat_n("start,data,stop", 9)
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(attribute(&dom, "data-events"), expected);
    assert!(
        bytes(&dom).is_empty(),
        "no capture means no predictive codec state"
    );
}

#[test]
fn changing_capture_rate_or_channels_finishes_prior_pcm_then_reports_not_supported() {
    for (rate, channels, frames) in [(24_000, 1, 480), (48_000, 2, 960)] {
        let (dom, mut runtime, request) = start("{mimeType: 'audio/ogg'}", "recorder.start();");
        capture(&mut runtime, request, 1, 48_000, 1, 960);
        capture(&mut runtime, request, 2, rate, channels, frames);
        run_queued_events(&mut runtime);
        assert_eq!(
            attribute(&dom, "data-events"),
            "start,NotSupportedError,data,stop"
        );
        let pcm = decoded(&dom, 1, 960);
        assert!(pcm.iter().any(|sample| sample.abs() > 0.01));
    }
}

#[test]
fn cancelling_document_retires_all_eight_native_recorders_and_queued_callbacks() {
    let (dom, mut runtime, request) = start("{mimeType: 'audio/ogg'}", "");
    run(
        &dom,
        &mut runtime,
        r#"
        window.liveRecorders = Array.from({length: 8}, () =>
            new MediaRecorder(stream, {mimeType: 'audio/ogg'}));
        for (const active of liveRecorders) {
            active.ondataavailable = () => document.body.setAttribute('data-late', 'unexpected');
            active.start();
        }
        document.body.setAttribute('data-active', String(liveRecorders.filter(r =>
            r.state === 'recording').length));
    "#,
    );
    assert_eq!(attribute(&dom, "data-active"), "8");
    capture(&mut runtime, request, 1, 48_000, 2, 960);
    assert!(
        runtime
            .host
            .borrow_mut()
            .media_recorders
            .open("opus", 128_000, false)
            .is_err()
    );
    runtime.cancel_document();
    assert_eq!(runtime.next_timer_delay(), None);
    assert_eq!(
        dom.elements_named("body").next().unwrap().attr("data-late"),
        None
    );
    let mut host = runtime.host.borrow_mut();
    let recovered = (0..8)
        .map(|_| {
            host.media_recorders
                .open("opus", 128_000, false)
                .expect("retired Opus slot")
        })
        .collect::<Vec<_>>();
    assert!(host.media_recorders.open("opus", 128_000, false).is_err());
    for id in recovered {
        host.media_recorders.cancel(id);
    }
}
