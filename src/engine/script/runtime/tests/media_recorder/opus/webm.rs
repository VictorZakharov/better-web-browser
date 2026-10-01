//! Verify the WebM container through the ordinary document capture lifecycle.

use super::*;

const WEBM: &str = "{mimeType:'audio/webm;codecs=opus'}";

#[test]
fn webm_chunk_concatenation_and_ogg_have_identical_predictive_pcm() {
    let mut outputs = Vec::new();
    for mime in ["audio/ogg", "audio/webm"] {
        let options = format!("{{mimeType:'{mime}',audioBitsPerSecond:64000}}");
        let (dom, mut runtime, request) = start(&options, "recorder.start(40);");
        for sequence in 1..=5 {
            capture(&mut runtime, request, sequence, 48_000, 2, 960);
            if sequence == 1 || sequence == 3 {
                run(&dom, &mut runtime, "recorder.requestData();");
            }
            run_queued_events(&mut runtime);
        }
        capture(&mut runtime, request, 6, 48_000, 2, 71);
        run(&dom, &mut runtime, "recorder.stop();");
        assert_eq!(attribute(&dom, "data-type"), format!("{mime};codecs=opus"));
        assert_eq!(attribute(&dom, "data-state"), "inactive");
        assert!(attribute(&dom, "data-events").starts_with("start,data,"));
        assert!(attribute(&dom, "data-events").ends_with(",data,stop"));
        let times = attribute(&dom, "data-timecodes")
            .split(',')
            .map(|time| time.parse::<f64>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(times[0], 0.0);
        assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
        outputs.push(decoded(&dom, 2, 4_871));
    }
    assert_eq!(
        outputs[0], outputs[1],
        "container choice must not change PCM"
    );
}

#[test]
fn webm_empty_request_data_does_not_restart_headers_or_change_final_duration() {
    let (dom, mut runtime, request) = start(WEBM, "recorder.start();");
    run(
        &dom,
        &mut runtime,
        "recorder.requestData(); recorder.requestData();",
    );
    capture(&mut runtime, request, 1, 48_000, 1, 17);
    run(
        &dom,
        &mut runtime,
        "recorder.requestData(); recorder.requestData();",
    );
    capture(&mut runtime, request, 2, 48_000, 1, 960);
    run(
        &dom,
        &mut runtime,
        "recorder.requestData(); recorder.stop();",
    );
    assert_eq!(
        attribute(&dom, "data-events"),
        "start,data,data,data,data,data,data,stop"
    );
    assert_eq!(attribute(&dom, "data-parts"), "6");
    let source = bytes(&dom);
    assert_eq!(
        source
            .windows(4)
            .filter(|part| *part == [0x1a, 0x45, 0xdf, 0xa3])
            .count(),
        1,
        "requestData must drain bytes rather than begin another document"
    );
    let pcm = decoded(&dom, 1, 977);
    assert!(pcm.iter().any(|sample| sample.abs() > 0.01));
}

#[test]
fn webm_pause_and_disabled_track_preserve_only_active_silent_duration() {
    let (dom, mut runtime, request) = start(WEBM, "recorder.start(); recorder.pause();");
    capture(&mut runtime, request, 1, 48_000, 1, 960);
    run(
        &dom,
        &mut runtime,
        "track.enabled=false; recorder.requestData(); recorder.resume();",
    );
    for sequence in 2..=4 {
        capture(&mut runtime, request, sequence, 48_000, 1, 960);
    }
    run(&dom, &mut runtime, "recorder.stop();");
    assert_eq!(
        attribute(&dom, "data-events"),
        "start,pause,data,resume,data,stop"
    );
    assert!(
        decoded(&dom, 1, 2_880)
            .iter()
            .all(|sample| sample.abs() < 0.0001)
    );
}

#[test]
fn webm_native_capture_rates_and_stereo_keep_fractional_terminal_samples() {
    for rate in [8_000, 12_000, 16_000, 24_000, 48_000] {
        for channels in [1, 2] {
            let (dom, mut runtime, request) = start(WEBM, "recorder.start();");
            capture(&mut runtime, request, 1, rate, channels, rate / 50);
            capture(&mut runtime, request, 2, rate, channels, 17);
            run(&dom, &mut runtime, "recorder.stop();");
            assert_eq!(attribute(&dom, "data-events"), "start,data,stop");
            let frames = u64::from(rate / 50 + 17) * 48_000 / u64::from(rate);
            let pcm = decoded(&dom, channels as u16, frames);
            assert!(pcm.iter().any(|sample| sample.abs() > 0.01));
        }
    }
}

#[test]
fn webm_incompatible_capture_finishes_valid_prefix_before_error_and_stop() {
    for (rate, channels, frames) in [(44_100, 1, 882), (24_000, 1, 480), (48_000, 2, 960)] {
        let (dom, mut runtime, request) = start(WEBM, "recorder.start();");
        capture(&mut runtime, request, 1, 48_000, 1, 17);
        capture(&mut runtime, request, 2, rate, channels, frames);
        run_queued_events(&mut runtime);
        assert_eq!(
            attribute(&dom, "data-events"),
            "start,NotSupportedError,data,stop"
        );
        assert_eq!(attribute(&dom, "data-state"), "inactive");
        decoded(&dom, 1, 17);
    }
}

#[test]
fn webm_unsupported_first_format_has_no_header_and_recorder_can_restart() {
    let (dom, mut runtime, request) = start(WEBM, "recorder.start();");
    capture(&mut runtime, request, 1, 44_100, 1, 882);
    run_queued_events(&mut runtime);
    assert!(bytes(&dom).is_empty());
    assert_eq!(
        attribute(&dom, "data-events"),
        "start,NotSupportedError,data,stop"
    );
    run(
        &dom,
        &mut runtime,
        "parts.length=0; events.length=0; recorder.start();",
    );
    capture(&mut runtime, request, 2, 48_000, 2, 960);
    run(&dom, &mut runtime, "recorder.stop();");
    assert_eq!(attribute(&dom, "data-events"), "start,data,stop");
    decoded(&dom, 2, 960);
}

#[test]
fn webm_track_removal_and_track_end_finalize_even_subpacket_recordings() {
    for remove in [false, true] {
        let (dom, mut runtime, request) = start(WEBM, "recorder.start();");
        capture(&mut runtime, request, 1, 48_000, 1, 17);
        if remove {
            run(&dom, &mut runtime, "stream.removeTrack(track);");
            assert_eq!(
                attribute(&dom, "data-events"),
                "start,InvalidModificationError,data,stop"
            );
        } else {
            let outcome = runtime.deliver_media_capture_update(MediaCaptureUpdate {
                document: DocumentId::new(1).unwrap(),
                request_id: request,
                event: MediaCaptureEvent::TrackEnded { track_id: 2 },
            });
            assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
            run_queued_events(&mut runtime);
            assert_eq!(attribute(&dom, "data-events"), "start,data,stop");
        }
        assert_eq!(attribute(&dom, "data-state"), "inactive");
        decoded(&dom, 1, 17);
    }
}

#[test]
fn webm_document_cancellation_releases_shared_native_recorder_quota() {
    let (dom, mut runtime, request) = start(WEBM, "");
    run(
        &dom,
        &mut runtime,
        r#"
        window.active = Array.from({length:8}, (_,index) => new MediaRecorder(stream,
            {mimeType:index % 2 ? 'audio/ogg' : 'audio/webm'}));
        for (const recorder of active) {
            recorder.ondataavailable=()=>document.body.setAttribute('data-late','bad');
            recorder.start();
        }
    "#,
    );
    capture(&mut runtime, request, 1, 48_000, 2, 960);
    assert!(
        runtime
            .host
            .borrow_mut()
            .media_recorders
            .open("webm-opus", 64_000, false)
            .is_err()
    );
    runtime.cancel_document();
    assert_eq!(runtime.next_timer_delay(), None);
    assert!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-late")
            .is_none()
    );
    let mut host = runtime.host.borrow_mut();
    let ids = (0..8)
        .map(|index| {
            host.media_recorders
                .open(
                    if index % 2 == 0 { "webm-opus" } else { "opus" },
                    64_000,
                    false,
                )
                .expect("both containers share and release one quota")
        })
        .collect::<Vec<_>>();
    assert!(
        host.media_recorders
            .open("webm-opus", 64_000, false)
            .is_err()
    );
    for id in ids {
        host.media_recorders.cancel(id);
    }
}
