use super::*;
#[test]
fn media_source_object_url_appends_bounded_muxed_bytes_and_ends() {
    let dom = dom::parse_with_scripting(
        r#"<body><output id="status">waiting</output><video id="movie"></video><script>
            const mediaSource = new MediaSource();
            const objectUrl = URL.createObjectURL(mediaSource);
            const events = [];
            let sourceBuffer;
            let appendWasAsync = false;
            let removalWasAsync = false;
            let phase = 'append';
            mediaSource.addEventListener('sourceopen', () => events.push('sourceopen'));
            mediaSource.addEventListener('sourceended', () => events.push('sourceended'));
            mediaSource.addEventListener('sourceopen', () => {
                const type = 'video/mp4; codecs="avc1.42E01E,mp4a.40.2"';
                sourceBuffer = mediaSource.addSourceBuffer(type);
                for (const eventName of ['updatestart', 'update', 'updateend']) {
                    sourceBuffer.addEventListener(eventName, () => events.push(eventName));
                }
                sourceBuffer.addEventListener('updateend', () => {
                    if (phase === 'append') {
                        phase = 'remove';
                        mediaSource.endOfStream();
                    } else {
                        document.getElementById('status').textContent = [
                            appendWasAsync,
                            removalWasAsync,
                            mediaSource.readyState,
                            sourceBuffer.buffered.start(0),
                            sourceBuffer.buffered.end(0),
                            movie.buffered.start(0),
                            movie.buffered.end(0),
                            events.join(',')
                        ].join(':');
                        URL.revokeObjectURL(objectUrl);
                    }
                });
                sourceBuffer.appendBuffer(new Uint8Array([0, 0, 0, 8, 109, 100, 97, 116]));
                appendWasAsync = sourceBuffer.updating && !events.includes('updatestart');
            }, { once: true });
            mediaSource.addEventListener('sourceended', () => {
                sourceBuffer.remove(0, 0.25);
                removalWasAsync = sourceBuffer.updating;
            });
            movie.src = objectUrl;
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#media-source".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[input]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "waiting"
    );
    let commit = outcome
        .media_actions
        .iter()
        .find_map(|action| match &action.command {
            ScriptMediaCommand::Commit { mime_type, bytes } => Some((mime_type, bytes)),
            _ => None,
        })
        .expect("MediaSource endOfStream did not commit its admitted bytes");
    assert!(commit.0.starts_with("video/mp4"));
    assert_eq!(commit.1, &[0, 0, 0, 8, 109, 100, 97, 116]);
    let video = dom.elements_named("video").next().unwrap();
    let loaded = runtime.dispatch_user_input(UserInputEvent::Media {
        buffered: None,
        target: video,
        request_id: 0,
        disposition: "loaded",
        current_time: 0.0,
        duration: 1.0,
        width: 320,
        height: 180,
    });
    assert!(
        loaded.outcome.errors.is_empty(),
        "{:?}",
        loaded.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        concat!(
            "true:true:open:0.25:1:0.25:1:",
            "sourceopen,updatestart,update,updateend,sourceended,",
            "sourceopen,updatestart,update,updateend"
        )
    );
}

#[test]
fn media_source_admits_separate_h264_and_aac_tracks_and_defers_play_until_loaded() {
    let dom = dom::parse_with_scripting(
        r#"<body><output id="status">waiting</output><video id="movie"></video><script>
            const mediaSource = new MediaSource();
            const movie = document.getElementById('movie');
            const videoType = 'video/mp4; codecs="avc1.4d401e"';
            const audioType = 'audio/mp4; codecs="mp4a.40.2"';
            const bytes = new Uint8Array([0, 0, 0, 8, 109, 100, 97, 116]);
            mediaSource.addEventListener('sourceopen', () => {
                const video = mediaSource.addSourceBuffer(videoType);
                const audio = mediaSource.addSourceBuffer(audioType);
                video.appendBuffer(bytes);
                audio.appendBuffer(bytes);
                movie.play().then(() => {
                    document.getElementById('status').textContent = 'playing';
                });
            }, { once: true });
            movie.src = URL.createObjectURL(mediaSource);
            document.getElementById('status').textContent = [
                MediaSource.isTypeSupported(videoType),
                MediaSource.isTypeSupported(audioType)
            ].join(':');
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#adaptive-media-source".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[input]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true:true"
    );
    let commit = outcome
        .media_actions
        .iter()
        .find_map(|action| match &action.command {
            ScriptMediaCommand::CommitAdaptive {
                video_mime_type,
                video_bytes,
                audio_mime_type,
                audio_bytes,
            } => Some((
                action.node,
                video_mime_type,
                video_bytes,
                audio_mime_type,
                audio_bytes,
            )),
            _ => None,
        })
        .expect("separate populated SourceBuffers did not commit after play");
    assert!(commit.1.starts_with("video/mp4"));
    assert!(commit.3.starts_with("audio/mp4"));
    assert_eq!(commit.2, &[0, 0, 0, 8, 109, 100, 97, 116]);
    assert_eq!(commit.4, &[0, 0, 0, 8, 109, 100, 97, 116]);
    assert!(
        !outcome
            .media_actions
            .iter()
            .any(|action| matches!(action.command, ScriptMediaCommand::SetPlayback { .. }))
    );
    let video = dom.elements_named("video").next().unwrap();
    assert_eq!(video.id(), commit.0);
    let loaded = runtime.dispatch_user_input(UserInputEvent::Media {
        buffered: None,
        target: video.clone(),
        request_id: 0,
        disposition: "loaded",
        current_time: 0.0,
        duration: 19.0,
        width: 640,
        height: 360,
    });
    assert!(
        loaded.outcome.errors.is_empty(),
        "{:?}",
        loaded.outcome.errors
    );
    let playback = loaded
        .outcome
        .media_actions
        .iter()
        .find(|action| {
            matches!(
                action.command,
                ScriptMediaCommand::SetPlayback { playing: true, .. }
            )
        })
        .expect("loaded adaptive source did not resume the deferred play request");
    let playing = runtime.dispatch_user_input(UserInputEvent::Media {
        buffered: None,
        target: video,
        request_id: playback.request_id,
        disposition: "playing",
        current_time: 0.0,
        duration: 19.0,
        width: 640,
        height: 360,
    });
    assert!(
        playing.outcome.errors.is_empty(),
        "{:?}",
        playing.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "playing"
    );
}

#[test]
fn media_source_waits_for_the_complete_declared_media_box() {
    let dom = dom::parse_with_scripting(
        r#"<body><video id="movie"></video><script>
            const mediaSource = new MediaSource();
            const movie = document.getElementById('movie');
            mediaSource.addEventListener('sourceopen', () => {
                const buffer = mediaSource.addSourceBuffer(
                    'video/mp4; codecs="avc1.4d401e, mp4a.40.2"');
                let appendedRemainder = false;
                buffer.addEventListener('updateend', () => {
                    if (appendedRemainder) return;
                    appendedRemainder = true;
                    buffer.appendBuffer(new Uint8Array([2, 3, 4]));
                });
                buffer.appendBuffer(new Uint8Array([
                    0, 0, 0, 12, 109, 100, 97, 116, 1
                ]));
                movie.play();
            }, { once: true });
            movie.src = URL.createObjectURL(mediaSource);
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#partial-media-box".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[input]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let committed = outcome
        .media_actions
        .iter()
        .find_map(|action| match &action.command {
            ScriptMediaCommand::Commit { bytes, .. } => Some(bytes),
            _ => None,
        })
        .expect("complete media box was not committed after its remaining bytes arrived");

    assert_eq!(committed, &[0, 0, 0, 12, 109, 100, 97, 116, 1, 2, 3, 4]);
}
