use super::*;
use crate::engine::script::{ScriptFetchOptions, ScriptKind, ScriptMediaCommand};
use crate::renderer_protocol::{DocumentId, MediaCaptureEvent, MediaCaptureUpdate};

#[test]
fn video_src_object_tracks_live_stream_membership_not_capture_request_identity() {
    let dom = dom::parse_with_scripting(
        r#"<body><video id="preview"></video><script>
            navigator.mediaDevices.getUserMedia({audio: true, video: true}).then(stream => {
                window.captureStream = stream;
                window.preview = document.getElementById('preview');
                window.audioOnly = new MediaStream([stream.getAudioTracks()[0]]);
                preview.srcObject = audioOnly;
            }).catch(error => document.body.setAttribute('data-error', String(error)));
        </script></body>"#,
        true,
    );
    let video = dom.elements_named("video").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let started = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: true,
            microphone: true,
        },
    });
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-error"),
        None
    );
    assert!(runtime.capture_video_nodes(request_id).is_empty());
    assert!(
        started.media_actions.iter().any(|action| {
            action.node == video.id()
                && matches!(
                    action.command,
                    ScriptMediaCommand::SelectVideo { selected: false }
                )
        }),
        "{:?}",
        started.media_actions
    );

    let evaluate = |runtime: &mut ScriptRuntime, code: &str| {
        runtime.execute_additional_with_loader(
            &[ScriptInput {
                source_url: "https://example.com/#capture-membership".into(),
                code: code.into(),
                node: dom.elements_named("script").next().unwrap(),
                kind: ScriptKind::Classic,
                fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
                finish_lifecycle: true,
            }],
            None,
        )
    };

    let added = evaluate(
        &mut runtime,
        "audioOnly.addTrack(captureStream.getVideoTracks()[0]);",
    );
    assert!(added.errors.is_empty(), "{:?}", added.errors);
    assert_eq!(
        runtime.capture_video_nodes(request_id),
        vec![(video.id(), true)]
    );
    let frame = runtime.deliver_media_capture_frame_info(request_id, 16, 9, 10_000_000);
    assert!(frame.errors.is_empty(), "{:?}", frame.errors);
    let removed = evaluate(
        &mut runtime,
        r#"audioOnly.removeTrack(captureStream.getVideoTracks()[0]);
           document.body.setAttribute('data-after-remove', [
               audioOnly.active, audioOnly.getAudioTracks().length,
               audioOnly.getVideoTracks().length, preview.videoWidth,
               preview.videoHeight, preview.srcObject === audioOnly
           ].join(':'));"#,
    );
    assert!(removed.errors.is_empty(), "{:?}", removed.errors);
    assert!(runtime.capture_video_nodes(request_id).is_empty());
    assert!(removed.media_actions.iter().any(|action| {
        action.node == video.id()
            && matches!(
                action.command,
                ScriptMediaCommand::SelectVideo { selected: false }
            )
    }));
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-after-remove")
            .as_deref(),
        Some("true:1:0:0:0:true")
    );
    let stale = runtime.deliver_media_capture_frame_info(request_id, 64, 48, 20_000_000);
    assert!(stale.errors.is_empty(), "{:?}", stale.errors);
    let observed = evaluate(
        &mut runtime,
        "document.body.setAttribute('data-stale-width', preview.videoWidth);",
    );
    assert!(observed.errors.is_empty(), "{:?}", observed.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-stale-width")
            .as_deref(),
        Some("0")
    );

    let restored = evaluate(
        &mut runtime,
        "audioOnly.addTrack(captureStream.getVideoTracks()[0]);",
    );
    assert!(restored.errors.is_empty(), "{:?}", restored.errors);
    assert_eq!(
        runtime.capture_video_nodes(request_id),
        vec![(video.id(), true)]
    );
    let stopped = evaluate(&mut runtime, "captureStream.getVideoTracks()[0].stop();");
    assert!(stopped.errors.is_empty(), "{:?}", stopped.errors);
    assert!(runtime.capture_video_nodes(request_id).is_empty());
}

#[test]
fn initially_empty_src_object_attaches_when_a_live_video_track_is_added() {
    let dom = dom::parse_with_scripting(
        r#"<body><video id="preview"></video><script>
            navigator.mediaDevices.getUserMedia({video: true}).then(stream => {
                window.captureStream = stream;
                window.empty = new MediaStream([]);
                document.getElementById('preview').srcObject = empty;
            });
        </script></body>"#,
        true,
    );
    let video = dom.elements_named("video").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let started = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: true,
            microphone: false,
        },
    });
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    assert!(runtime.capture_video_nodes(request_id).is_empty());
    let added = runtime.execute_additional_with_loader(
        &[ScriptInput {
            source_url: "https://example.com/#add-to-empty".into(),
            code: "empty.addTrack(captureStream.getVideoTracks()[0]);".into(),
            node: dom.elements_named("script").next().unwrap(),
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        }],
        None,
    );
    assert!(added.errors.is_empty(), "{:?}", added.errors);
    assert_eq!(
        runtime.capture_video_nodes(request_id),
        vec![(video.id(), true)]
    );
}
