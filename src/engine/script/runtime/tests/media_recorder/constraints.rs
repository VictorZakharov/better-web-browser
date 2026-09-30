use super::*;

#[test]
fn recorder_rejects_unsupported_formats_and_changed_track_sets() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        const supported = ['', 'audio/flac', 'audio/x-flac', 'audio/ogg',
            'video/webm', 'audio/flac;codecs=flac', ' ',
            'audio/webm;codecs=opus', 'audio/ogg;codecs=opus',
            'audio/ogg;codecs=speex'].map(type =>
            MediaRecorder.isTypeSupported(type));
        document.body.setAttribute('data-supported', supported.join(','));
        try { MediaRecorder.isTypeSupported(); }
        catch (error) { document.body.setAttribute('data-missing-type', error.name); }
        navigator.mediaDevices.getUserMedia({audio: true}).then(stream => {
            try { new MediaRecorder(stream, {mimeType: 'video/webm'}); }
            catch (error) { document.body.setAttribute('data-mime-error', error.name); }
            try { new MediaRecorder(stream, {mimeType: Symbol('audio/flac')}); }
            catch (error) { document.body.setAttribute('data-symbol-error', error.name); }
            window.recorder = new MediaRecorder(stream);
            recorder.onstart = () => window.events.push('start');
            recorder.onerror = event => window.events.push(event.error.name);
            recorder.ondataavailable = () => window.events.push('data');
            recorder.onstop = () => {
                window.events.push('stop');
                document.body.setAttribute('data-events', window.events.join(','));
            };
            window.events = [];
            recorder.start();
            stream.removeTrack(stream.getAudioTracks()[0]);
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
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-supported").as_deref(),
        Some("true,true,false,true,false,false,false,false,true,false")
    );
    assert_eq!(
        body.attr("data-mime-error").as_deref(),
        Some("NotSupportedError")
    );
    assert_eq!(body.attr("data-missing-type").as_deref(), Some("TypeError"));
    assert_eq!(body.attr("data-symbol-error").as_deref(), Some("TypeError"));
    assert_eq!(body.attr("data-state").as_deref(), Some("inactive"));
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some("start,InvalidModificationError,data,stop")
    );
}

#[test]
fn recorder_rejects_video_and_multitrack_streams_and_skips_paused_pcm() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        navigator.mediaDevices.getUserMedia({audio: true, video: true}).then(stream => {
            const audioTrack = stream.getAudioTracks()[0];
            const videoOnly = new MediaStream(stream.getVideoTracks());
            for (const [name, candidate] of [['mixed', stream], ['video', videoOnly]]) {
                try { new MediaRecorder(candidate).start(); }
                catch (error) { document.body.setAttribute('data-' + name, error.name); }
            }
            window.audioTrack = audioTrack;
            window.recorder = new MediaRecorder(new MediaStream([audioTrack]));
            window.parts = [];
            window.events = [];
            for (const type of ['start', 'pause', 'resume', 'stop'])
                recorder.addEventListener(type, () => events.push(type));
            recorder.ondataavailable = event => { events.push('data'); parts.push(event.data); };
            recorder.onstop = () => {
                new Blob(parts).bytes().then(bytes => {
                    document.body.setAttribute('data-flac', btoa(String.fromCharCode(...bytes)));
                    document.body.setAttribute('data-events', events.join(','));
                });
            };
            recorder.start();
            recorder.pause();
            document.body.setAttribute('data-paused-state', recorder.state);
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
            camera: true,
            microphone: true,
        },
    });
    assert!(granted.errors.is_empty(), "{:?}", granted.errors);
    run_queued_events(&mut runtime);
    deliver_audio(&mut runtime, request_id, 1);
    let resumed = runtime.execute_additional_with_loader(
        &[input(
            &node,
            "resume-recorder.js",
            "recorder.resume(); audioTrack.enabled = false;",
            true,
        )],
        None,
    );
    assert!(resumed.errors.is_empty(), "{:?}", resumed.errors);
    deliver_audio(&mut runtime, request_id, 2);
    let enabled = runtime.execute_additional_with_loader(
        &[input(
            &node,
            "enable-recorder.js",
            "audioTrack.enabled = true; recorder.stop();",
            true,
        )],
        None,
    );
    assert!(enabled.errors.is_empty(), "{:?}", enabled.errors);
    run_queued_events(&mut runtime);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-mixed").as_deref(),
        Some("NotSupportedError")
    );
    assert_eq!(
        body.attr("data-video").as_deref(),
        Some("NotSupportedError")
    );
    assert_eq!(body.attr("data-paused-state").as_deref(), Some("paused"));
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some("start,pause,resume,data,stop")
    );
    let encoded = base64::engine::general_purpose::STANDARD
        .decode(body.attr("data-flac").unwrap())
        .unwrap();
    let decoded = claxon::FlacReader::new(Cursor::new(encoded))
        .unwrap()
        .samples()
        .map(Result::unwrap)
        .collect::<Vec<i32>>();
    assert_eq!(decoded, vec![0; 160]);
}
