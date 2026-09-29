use super::*;
use crate::engine::script::{ScriptFetchOptions, ScriptGraphAudioAction, ScriptKind};
use crate::renderer_protocol::{
    DocumentId, MediaCaptureAction, MediaCaptureError, MediaCaptureEvent, MediaCaptureFrame,
    MediaCaptureFrameKind, MediaCaptureUpdate,
};
use std::time::Duration;

#[test]
fn capture_starts_only_after_browser_grant_and_stops_tracks_separately() {
    let dom = dom::parse_with_scripting(
        r#"<body><video id="preview"></video><script>
            navigator.mediaDevices.getUserMedia({audio: true, video: true}).then(stream => {
                const video = document.getElementById('preview');
                video.srcObject = stream;
                document.body.setAttribute('data-started', String(
                    stream instanceof MediaStream && stream.active &&
                    stream.getAudioTracks().length === 1 &&
                    stream.getVideoTracks().length === 1 &&
                    video.srcObject === stream));
                stream.getVideoTracks()[0].stop();
                document.body.setAttribute('data-audio-live', String(
                    stream.getAudioTracks()[0].readyState === 'live' && stream.active));
            });
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request = &initial.media_device_actions[0];
    assert_eq!(
        request.capture,
        Some(MediaCaptureAction::Start {
            camera: true,
            microphone: true,
        })
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-started"),
        None
    );

    let result = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: request.request_id,
        event: MediaCaptureEvent::Started {
            camera: true,
            microphone: true,
        },
    });
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-started").as_deref(), Some("true"));
    assert_eq!(body.attr("data-audio-live").as_deref(), Some("true"));
    assert!(
        result
            .media_device_actions
            .iter()
            .any(|action| action.request_id == request.request_id
                && action.capture == Some(MediaCaptureAction::Stop { track_id: 1 }))
    );
}

#[test]
fn denied_capture_rejects_and_invalid_constraints_do_not_request_hardware() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            navigator.mediaDevices.getUserMedia({video: false}).catch(error =>
                document.body.setAttribute('data-invalid', error.name));
            navigator.mediaDevices.getUserMedia({video: {deviceId: {exact: 'secret'}}})
                .catch(error => document.body.setAttribute('data-constraint', error.name));
            navigator.mediaDevices.getUserMedia({video: {width: {min: 1920}}})
                .catch(error => document.body.setAttribute('data-minimum', error.name));
            navigator.mediaDevices.getUserMedia({video: true}).catch(error =>
                document.body.setAttribute('data-denied', error.name));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.media_device_actions.len(), 1);
    let result = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: initial.media_device_actions[0].request_id,
        event: MediaCaptureEvent::Error(MediaCaptureError::NotAllowed),
    });
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-invalid").as_deref(), Some("TypeError"));
    assert_eq!(
        body.attr("data-constraint").as_deref(),
        Some("OverconstrainedError")
    );
    assert_eq!(
        body.attr("data-minimum").as_deref(),
        Some("OverconstrainedError")
    );
    assert_eq!(body.attr("data-denied").as_deref(), Some("NotAllowedError"));
}

#[test]
fn unsupported_advanced_capture_preferences_fall_back_to_default_device() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            navigator.mediaDevices.getUserMedia({video: {
                advanced: [{deviceId: {exact: 'missing-camera'}}, {width: {min: 4096}}]
            }}).then(stream => document.body.setAttribute('data-result', String(
                stream.getVideoTracks().length === 1)),
                error => document.body.setAttribute('data-result', error.name));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.media_device_actions.len(), 1);
    let request = &initial.media_device_actions[0];
    assert_eq!(
        request.capture,
        Some(MediaCaptureAction::Start {
            camera: true,
            microphone: false,
        })
    );

    let result = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: request.request_id,
        event: MediaCaptureEvent::Started {
            camera: true,
            microphone: false,
        },
    });
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true")
    );
}

#[test]
fn insecure_origin_cannot_expose_capture_devices() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            document.body.setAttribute('data-capture',
                String(navigator.mediaDevices?.getUserMedia));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "http://example.com/");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-capture")
            .as_deref(),
        Some("undefined")
    );
}

#[test]
fn page_script_cannot_forge_capture_control_or_sample_callbacks() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            document.body.setAttribute('data-private', String(
                typeof __receiveMediaCaptureUpdate === 'undefined' &&
                typeof __receiveMediaCaptureFrame === 'undefined' &&
                typeof __receiveMediaCaptureAudioFrame === 'undefined'));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-private")
            .as_deref(),
        Some("true")
    );
}

#[test]
fn native_microphone_pcm_reaches_a_connected_web_audio_source() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            navigator.mediaDevices.getUserMedia({audio: true}).then(stream => {
                window.context = new AudioContext({sampleRate: 8000});
                window.source = context.createMediaStreamSource(stream);
                source.connect(context.destination);
            });
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let started = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: false,
            microphone: true,
        },
    });
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    let bytes = (0..160).flat_map(|_| 4096_i16.to_le_bytes()).collect();
    let delivered = runtime.deliver_media_capture_audio_frame(MediaCaptureFrame {
        document: DocumentId::new(1).unwrap(),
        request_id,
        track_id: 2,
        sequence: 1,
        timestamp_100ns: 0,
        kind: MediaCaptureFrameKind::AudioPcm16,
        width_or_rate: 8000,
        height_or_frames: 160,
        stride_or_channels: 1,
        bytes,
    });
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    runtime.set_audio_activation(true);
    let activation = runtime.execute_additional_with_loader(
        &[ScriptInput {
            source_url: "https://example.com/#activate-capture".into(),
            code: "__notifyAudioActivation();".into(),
            node: dom.elements_named("script").next().unwrap(),
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        }],
        None,
    );
    assert!(activation.errors.is_empty(), "{:?}", activation.errors);
    let mut got_pcm = false;
    for _ in 0..10 {
        let next = runtime.advance_time(Duration::from_millis(10), 1);
        assert!(next.errors.is_empty(), "{:?}", next.errors);
        for action in next.graph_audio_actions {
            if let ScriptGraphAudioAction::Queue { pcm, .. } = action {
                assert_eq!(i16::from_le_bytes([pcm[0], pcm[1]]), 4096);
                got_pcm = true;
                break;
            }
        }
        if got_pcm {
            break;
        }
    }
    assert!(
        got_pcm,
        "native PCM did not enter the connected Web Audio graph"
    );
}

#[test]
fn capture_stop_is_delivered_when_enumeration_fills_the_task_queue() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            navigator.mediaDevices.getUserMedia({audio: true})
                .then(stream => { window.captureStream = stream; });
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let started = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: false,
            microphone: true,
        },
    });
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    let stopped = runtime.execute_additional_with_loader(
        &[ScriptInput {
            source_url: "https://example.com/#stop-under-pressure".into(),
            code: r#"
                for (let i = 0; i < 64; ++i) navigator.mediaDevices.enumerateDevices();
                captureStream.getAudioTracks()[0].stop();
                document.body.setAttribute('data-ended',
                    captureStream.getAudioTracks()[0].readyState);
            "#
            .into(),
            node: dom.elements_named("script").next().unwrap(),
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        }],
        None,
    );
    assert!(stopped.errors.is_empty(), "{:?}", stopped.errors);
    assert_eq!(stopped.media_device_actions.len(), 65);
    assert_eq!(
        stopped.media_device_actions.last().unwrap().capture,
        Some(MediaCaptureAction::Stop { track_id: 2 })
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-ended")
            .as_deref(),
        Some("ended")
    );
}

#[test]
fn browser_revocation_rejects_pending_capture_and_ends_live_tracks() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            navigator.mediaDevices.getUserMedia({audio: true}).then(stream => {
                const track = stream.getAudioTracks()[0];
                track.addEventListener('ended', () => document.body.setAttribute(
                    'data-ended', `${track.readyState}:${stream.active}`));
                document.body.setAttribute('data-started', track.readyState);
            });
            navigator.mediaDevices.getUserMedia({video: true}).then(
                () => document.body.setAttribute('data-pending', 'started'),
                error => document.body.setAttribute('data-pending', error.name));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.media_device_actions.len(), 2);
    let document = DocumentId::new(1).unwrap();
    let started = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document,
        request_id: initial.media_device_actions[0].request_id,
        event: MediaCaptureEvent::Started {
            camera: false,
            microphone: true,
        },
    });
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    let aborted = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document,
        request_id: initial.media_device_actions[1].request_id,
        event: MediaCaptureEvent::Error(MediaCaptureError::Abort),
    });
    assert!(aborted.errors.is_empty(), "{:?}", aborted.errors);
    let ended = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document,
        request_id: initial.media_device_actions[0].request_id,
        event: MediaCaptureEvent::Ended,
    });
    assert!(ended.errors.is_empty(), "{:?}", ended.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-started").as_deref(), Some("live"));
    assert_eq!(body.attr("data-pending").as_deref(), Some("AbortError"));
    assert_eq!(body.attr("data-ended").as_deref(), Some("ended:false"));
}
