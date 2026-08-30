use super::*;

#[test]
fn exposes_closed_truthful_html_media_bindings() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><video id="movie" controls playsinline>
            <source src="movie.mp4" type="video/mp4; codecs=&quot;avc1.42E01E&quot;">
        </video><script>
            const video = document.getElementById('movie');
            const source = video.querySelector('source');
            const audio = new Audio('/sound.mp4');
            const checks = [
                video instanceof HTMLVideoElement,
                video instanceof HTMLMediaElement,
                audio instanceof HTMLAudioElement,
                audio.src === 'https://example.com/sound.mp4',
                source instanceof HTMLSourceElement,
                source.type.startsWith('video/mp4'),
                video.controls && video.playsInline,
                video.networkState === HTMLMediaElement.NETWORK_EMPTY,
                video.readyState === HTMLMediaElement.HAVE_NOTHING,
                video.paused && !video.ended && !video.seeking,
                Number.isNaN(video.duration),
                video.currentSrc === '',
                video.buffered instanceof TimeRanges && video.buffered.length === 0,
                video.canPlayType(source.type) === 'probably',
                video.canPlayType('video/webm; codecs="vp9"') === '',
                MediaError.MEDIA_ERR_DECODE === 3,
                'onloadedmetadata' in video && 'ontimeupdate' in video
            ];
            video.volume = 0.5;
            video.muted = true;
            checks.push(video.volume === 0.5 && video.muted);
            document.getElementById('status').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn media_methods_fail_closed_and_validate_ranges() {
    let dom = dom::parse_with_scripting(
        r#"<body><div id="status">waiting</div><video id="movie"></video><script>
            const video = document.getElementById('movie');
            let volumeError = '';
            let rangeError = '';
            try { video.volume = 2; } catch (error) { volumeError = error.name; }
            try { video.buffered.start(0); } catch (error) { rangeError = error.name; }
            video.play().then(
                () => document.getElementById('status').textContent = 'played',
                error => document.getElementById('status').textContent = [
                    error.name, volumeError, rangeError, video.paused
                ].join(',')
            );
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#media".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[input]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.media_actions.len(), 1);
    let action = &outcome.media_actions[0];
    assert!(matches!(
        action.command,
        ScriptMediaCommand::SetPlayback { playing: true, .. }
    ));
    let video = dom.elements_named("video").next().unwrap();
    let response = runtime.dispatch_user_input(UserInputEvent::Media {
        target: video,
        request_id: action.request_id,
        disposition: "denied",
        current_time: 0.0,
        duration: f64::NAN,
        width: 0,
        height: 0,
    });
    assert!(
        response.outcome.errors.is_empty(),
        "{:?}",
        response.outcome.errors
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "NotSupportedError,IndexSizeError,IndexSizeError,true"
    );
}

#[test]
fn unsupported_codecs_and_encrypted_media_fail_closed() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="status">waiting</output><video id="movie"></video><script>
            const movie = document.getElementById('movie');
            const results = [];
            const source = new MediaSource();
            const objectUrl = URL.createObjectURL(source);
            const unsupportedCodec = new Promise(resolve => {
                source.addEventListener('sourceopen', () => {
                    try {
                        source.addSourceBuffer('video/webm; codecs="vp09.00.10.08"');
                    } catch (error) {
                        results.push('codec:' + error.name);
                    }
                    URL.revokeObjectURL(objectUrl);
                    resolve();
                }, { once: true });
            });
            movie.src = objectUrl;
            Promise.all([
                unsupportedCodec,
                navigator.requestMediaKeySystemAccess('com.widevine.alpha', []).then(
                    () => results.push('navigator:accepted'),
                    error => results.push('navigator:' + error.name)
                ),
                movie.setMediaKeys(null).then(
                    () => results.push('element:accepted'),
                    error => results.push('element:' + error.name)
                )
            ]).then(() => {
                results.push('mediaKeys:' + (movie.mediaKeys === null));
                results.push('handler:' + ('onencrypted' in movie));
                document.getElementById('status').textContent = results.join(',');
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        concat!(
            "codec:NotSupportedError,navigator:NotSupportedError,",
            "element:NotSupportedError,mediaKeys:true,handler:true"
        )
    );
}

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
                sourceBuffer.appendBuffer(new Uint8Array([0, 1, 2, 3]));
                appendWasAsync = sourceBuffer.updating && !events.includes('updatestart');
            }, { once: true });
            movie.addEventListener('loadedmetadata', () => {
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
    assert_eq!(commit.1, &[0, 1, 2, 3]);
    let video = dom.elements_named("video").next().unwrap();
    let loaded = runtime.dispatch_user_input(UserInputEvent::Media {
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
