use super::*;
use crate::renderer_protocol::{
    DocumentId, MediaCaptureEvent, MediaCaptureFrame, MediaCaptureFrameKind, MediaCaptureUpdate,
};
use base64::Engine;
use std::io::Cursor;

mod constraints;

fn deliver_audio_at(
    runtime: &mut ScriptRuntime,
    request_id: u64,
    sequence: u64,
    timestamp_100ns: u64,
) {
    let samples = (0..160)
        .map(|frame| (sequence as i16 * 100) + frame as i16)
        .collect::<Vec<_>>();
    let bytes = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    let outcome = runtime.deliver_media_capture_audio_frame(MediaCaptureFrame {
        document: DocumentId::new(1).unwrap(),
        request_id,
        track_id: 2,
        sequence,
        timestamp_100ns,
        kind: MediaCaptureFrameKind::AudioPcm16,
        width_or_rate: 8_000,
        height_or_frames: 160,
        stride_or_channels: 1,
        bytes,
    });
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

fn deliver_audio(runtime: &mut ScriptRuntime, request_id: u64, sequence: u64) {
    deliver_audio_at(runtime, request_id, sequence, (sequence - 1) * 200_000);
}

fn run_queued_events(runtime: &mut ScriptRuntime) {
    let outcome = runtime.advance_time(Duration::ZERO, 16);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn browser_pcm_round_trips_across_request_data_and_timeslice_blobs() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        window.order = [];
        window.parts = [];
        navigator.mediaDevices.getUserMedia({audio: true}).then(stream => {
            window.recorder = new MediaRecorder(stream, {mimeType: 'audio/flac'});
            recorder.onstart = () => order.push('start');
            recorder.ondataavailable = event => {
                order.push('data');
                parts.push(event.data);
                if (parts.length === 1)
                    document.body.setAttribute('data-first-timecode', String(event.timecode));
                document.body.setAttribute('data-type', event.data.type);
            };
            recorder.onstop = () => {
                order.push('stop');
                new Blob(parts, {type: 'audio/flac'}).bytes().then(bytes => {
                    document.body.setAttribute('data-flac',
                        btoa(String.fromCharCode(...bytes)));
                    document.body.setAttribute('data-order', order.join(','));
                    document.body.setAttribute('data-parts', String(parts.length));
                    document.body.setAttribute('data-state', recorder.state);
                });
            };
            recorder.start(100);
        });
    </script></body>"#,
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let granted = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: false,
            microphone: true,
        },
    });
    assert!(granted.errors.is_empty(), "{:?}", granted.errors);
    run_queued_events(&mut runtime);

    for sequence in 1..=2 {
        deliver_audio(&mut runtime, request_id, sequence);
    }
    let requested = runtime.execute_additional_with_loader(
        &[input(
            &node,
            "request-data.js",
            "recorder.requestData();",
            true,
        )],
        None,
    );
    assert!(requested.errors.is_empty(), "{:?}", requested.errors);
    run_queued_events(&mut runtime);

    for sequence in 3..=5 {
        deliver_audio(&mut runtime, request_id, sequence);
    }
    run_queued_events(&mut runtime);
    deliver_audio(&mut runtime, request_id, 6);
    let stopped = runtime.execute_additional_with_loader(
        &[input(&node, "stop-recorder.js", "recorder.stop();", true)],
        None,
    );
    assert!(stopped.errors.is_empty(), "{:?}", stopped.errors);
    run_queued_events(&mut runtime);

    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-order").as_deref(),
        Some("start,data,data,data,stop")
    );
    assert_eq!(body.attr("data-parts").as_deref(), Some("3"));
    assert_eq!(body.attr("data-type").as_deref(), Some("audio/flac"));
    assert_eq!(body.attr("data-state").as_deref(), Some("inactive"));
    assert_eq!(body.attr("data-first-timecode").as_deref(), Some("0"));
    let encoded = base64::engine::general_purpose::STANDARD
        .decode(body.attr("data-flac").expect("recorded FLAC bytes"))
        .unwrap();
    let decoded = claxon::FlacReader::new(Cursor::new(encoded))
        .unwrap()
        .samples()
        .map(Result::unwrap)
        .collect::<Vec<i32>>();
    let expected = (1..=6)
        .flat_map(|sequence| (0..160).map(move |frame| sequence * 100 + frame))
        .collect::<Vec<i32>>();
    assert_eq!(decoded, expected);
}

#[test]
fn recorder_preserves_a_dropped_capture_packet_as_timed_silence() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        window.parts = [];
        window.events = [];
        window.timecodes = [];
        navigator.mediaDevices.getUserMedia({audio: true}).then(stream => {
            window.recorder = new MediaRecorder(stream, {mimeType: 'audio/flac'});
            recorder.onstart = () => events.push('start');
            recorder.ondataavailable = event => {
                events.push('data');
                parts.push(event.data);
                timecodes.push(event.timecode);
            };
            recorder.onstop = () => {
                events.push('stop');
                new Blob(parts).bytes().then(bytes => {
                    document.body.setAttribute('data-flac',
                        btoa(String.fromCharCode(...bytes)));
                    document.body.setAttribute('data-events', events.join(','));
                    document.body.setAttribute('data-timecodes', timecodes.join(','));
                });
            };
            recorder.start();
        });
    </script></body>"#,
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let granted = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: false,
            microphone: true,
        },
    });
    assert!(granted.errors.is_empty(), "{:?}", granted.errors);
    run_queued_events(&mut runtime);
    deliver_audio(&mut runtime, request_id, 1);
    let requested = runtime.execute_additional_with_loader(
        &[input(
            &node,
            "request-data.js",
            "recorder.requestData();",
            true,
        )],
        None,
    );
    assert!(requested.errors.is_empty(), "{:?}", requested.errors);
    run_queued_events(&mut runtime);
    // The broker's oldest-drop policy removes sequence 2; its timestamp still
    // tells the recorder the exact 160-frame gap to represent in FLAC.
    deliver_audio(&mut runtime, request_id, 3);
    let stopped = runtime.execute_additional_with_loader(
        &[input(&node, "stop-recorder.js", "recorder.stop();", true)],
        None,
    );
    assert!(stopped.errors.is_empty(), "{:?}", stopped.errors);
    run_queued_events(&mut runtime);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some("start,data,data,stop")
    );
    assert_eq!(body.attr("data-timecodes").as_deref(), Some("0,20"));
    let encoded = base64::engine::general_purpose::STANDARD
        .decode(body.attr("data-flac").unwrap())
        .unwrap();
    let decoded = claxon::FlacReader::new(Cursor::new(encoded))
        .unwrap()
        .samples()
        .map(Result::unwrap)
        .collect::<Vec<i32>>();
    let expected = (0..160)
        .map(|frame| 100 + frame)
        .chain(std::iter::repeat_n(0, 160))
        .chain((0..160).map(|frame| 300 + frame))
        .collect::<Vec<i32>>();
    assert_eq!(decoded, expected);
}

#[test]
fn contiguous_capture_sequences_ignore_wall_clock_timestamp_jitter() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        navigator.mediaDevices.getUserMedia({audio: true}).then(stream => {
            window.recorder = new MediaRecorder(stream);
            recorder.ondataavailable = event => {
                event.data.bytes().then(bytes => document.body.setAttribute('data-flac',
                    btoa(String.fromCharCode(...bytes))));
            };
            recorder.onerror = event =>
                document.body.setAttribute('data-error', event.error.name);
            recorder.start();
        });
    </script></body>"#,
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let granted = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: false,
            microphone: true,
        },
    });
    assert!(granted.errors.is_empty(), "{:?}", granted.errors);
    run_queued_events(&mut runtime);
    deliver_audio_at(&mut runtime, request_id, 1, 0);
    // A delayed callback timestamp is not evidence of missing media when the
    // capture sequence is contiguous.
    deliver_audio_at(&mut runtime, request_id, 2, 500_000);
    let stopped = runtime.execute_additional_with_loader(
        &[input(&node, "stop-recorder.js", "recorder.stop();", true)],
        None,
    );
    assert!(stopped.errors.is_empty(), "{:?}", stopped.errors);
    run_queued_events(&mut runtime);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-error"), None);
    let encoded = base64::engine::general_purpose::STANDARD
        .decode(body.attr("data-flac").unwrap())
        .unwrap();
    let decoded = claxon::FlacReader::new(Cursor::new(encoded))
        .unwrap()
        .samples()
        .map(Result::unwrap)
        .collect::<Vec<i32>>();
    let expected = (1..=2)
        .flat_map(|sequence| (0..160).map(move |frame| sequence * 100 + frame))
        .collect::<Vec<i32>>();
    assert_eq!(decoded, expected);
}

#[test]
fn immediate_stop_and_restart_retires_native_slots_before_queued_events() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        navigator.mediaDevices.getUserMedia({audio: true}).then(stream => {
            const recorder = new MediaRecorder(stream);
            const events = [];
            recorder.onstart = () => events.push('start');
            recorder.ondataavailable = event => {
                if (event.data.size !== 0) throw Error('Unexpected capture bytes');
                events.push('data');
            };
            recorder.onstop = () => {
                events.push('stop');
                document.body.setAttribute('data-events', events.join(','));
            };
            for (let i = 0; i < 9; ++i) {
                recorder.start();
                recorder.stop();
            }
            document.body.setAttribute('data-state', recorder.state);
        });
    </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let granted = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: false,
            microphone: true,
        },
    });
    assert!(granted.errors.is_empty(), "{:?}", granted.errors);
    run_queued_events(&mut runtime);
    run_queued_events(&mut runtime);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-state").as_deref(), Some("inactive"));
    let expected = std::iter::repeat_n("start,data,stop", 9)
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(body.attr("data-events").as_deref(), Some(expected.as_str()));
}
