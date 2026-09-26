use super::*;

#[test]
fn exposes_closed_truthful_html_media_bindings() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><video id="movie" controls playsinline>
            <source src="movie.mp4" type="video/mp4; codecs=&quot;avc1.42E01E, mp4a.40.2&quot;">
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
                video.canPlayType('video/mp4; codecs="avc1.42E01E"') === 'probably',
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
fn can_play_type_reports_only_decodeable_complete_source_audio_and_video() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="status">waiting</output><script>
            const audio = document.createElement('audio');
            const video = document.createElement('video');
            const cases = [
                [audio, 'audio/wav', 'maybe'],
                [audio, 'audio/x-wav; codecs="1"', 'probably'],
                [audio, 'audio/mpeg', 'maybe'],
                [audio, 'audio/mpeg; codecs="mp3"', 'probably'],
                [audio, 'audio/aac', 'maybe'],
                [audio, 'audio/aac; codecs="mp4a.40.2"', 'probably'],
                [audio, 'audio/mp4', 'maybe'],
                [audio, 'audio/mp4; codecs="mp4a.40.2"', 'probably'],
                [video, 'video/mp4', 'maybe'],
                [video, 'video/mp4; codecs="avc1.42E01E, mp4a.40.2"', 'probably'],
                [video, 'video/mp4; codecs="avc1.42E01E"', 'probably'],
                [audio, 'audio/mp4; codecs="avc1.42E01E"', ''],
                [audio, 'audio/mp4; codecs="mp4a.40.5"', ''],
                [audio, 'audio/ogg; codecs="opus"', ''],
                [video, 'video/webm; codecs="vp9"', ''],
                [audio, 'audio/mp4; codecs="mp4a.40.2"; codecs="mp4a.40.2"', ''],
                [audio, 'audio/mp4; codecs', ''],
                [audio, 'application/octet-stream', '']
            ];
            const failure = cases.find(([element, type, expected]) =>
                element.canPlayType(type) !== expected);
            document.getElementById('status').textContent = failure
                ? JSON.stringify([failure[1], failure[2], failure[0].canPlayType(failure[1])])
                : 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
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
        buffered: None,
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
fn ordinary_audio_play_waits_for_loaded_data_and_pause_aborts_the_pending_promise() {
    let dom = dom::parse_with_scripting(
        r#"<body><output id="status">waiting</output><audio id="sound" src="/sound.mp3"></audio>
        <script>
            const sound = document.getElementById('sound');
            sound.play().then(
                () => document.getElementById('status').textContent = 'playing',
                error => document.getElementById('status').textContent = error.name
            );
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#audio".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[input]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome.media_actions.is_empty(),
        "play must wait for decode"
    );
    let sound = dom.elements_named("audio").next().unwrap();
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "waiting"
    );

    let loaded = runtime.dispatch_user_input(UserInputEvent::Media {
        buffered: None,
        target: sound.clone(),
        request_id: 0,
        disposition: "loaded",
        current_time: 0.0,
        duration: 3.0,
        width: 0,
        height: 0,
    });
    assert!(
        loaded.outcome.errors.is_empty(),
        "{:?}",
        loaded.outcome.errors
    );
    let action = loaded
        .outcome
        .media_actions
        .iter()
        .find(|action| {
            matches!(
                action.command,
                ScriptMediaCommand::SetPlayback { playing: true, .. }
            )
        })
        .expect("loaded data must release the deferred play request");
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "waiting"
    );
    let playing = runtime.dispatch_user_input(UserInputEvent::Media {
        buffered: None,
        target: sound,
        request_id: action.request_id,
        disposition: "playing",
        current_time: 0.0,
        duration: 3.0,
        width: 0,
        height: 0,
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
fn pausing_before_ordinary_audio_load_prevents_late_playback() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="status">waiting</output><audio id="sound" src="/sound.mp3"></audio>
        <script>
            const sound = document.getElementById('sound');
            sound.play().catch(error => document.getElementById('status').textContent = error.name);
            sound.pause();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(!outcome.media_actions.iter().any(|action| matches!(
        action.command,
        ScriptMediaCommand::SetPlayback { playing: true, .. }
    )));
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "AbortError"
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
            "navigator:NotSupportedError,element:NotSupportedError,",
            "codec:NotSupportedError,mediaKeys:true,handler:true"
        )
    );
}

#[test]
fn media_capabilities_reports_only_the_owned_decode_path() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="status">waiting</output><script>
        const video = {
            contentType: 'video/mp4; codecs="avc1.42E01E"',
            width: 320, height: 240, bitrate: 500000, framerate: 30
        };
        const audio = {
            contentType: 'audio/mp4; codecs="mp4a.40.2"',
            channels: '2', bitrate: 128000, samplerate: 48000
        };
        Promise.all([
            navigator.mediaCapabilities.decodingInfo({ type: 'file', video, audio }),
            navigator.mediaCapabilities.decodingInfo({ type: 'file', video }),
            navigator.mediaCapabilities.decodingInfo({
                type: 'file', audio: {
                    contentType: 'audio/mpeg; codecs="mp3"',
                    channels: '2', bitrate: 128000, samplerate: 44100
                }
            }),
            navigator.mediaCapabilities.decodingInfo({
                type: 'file', audio: {
                    contentType: 'audio/ogg; codecs="opus"',
                    channels: '2', bitrate: 128000, samplerate: 48000
                }
            }),
            navigator.mediaCapabilities.decodingInfo({
                type: 'media-source',
                video: { ...video, contentType: 'video/webm; codecs="vp09.00.10.08"' }
            }),
            navigator.mediaCapabilities.decodingInfo({
                type: 'media-source', video, keySystemConfiguration: { keySystem: 'widevine' }
            }),
            navigator.mediaCapabilities.decodingInfo({ type: 'file' }).then(
                () => 'accepted', error => error.name
            )
        ]).then(([owned, videoOnly, audioOnly, unsupportedAudio, unsupported, encrypted, invalid]) => {
            document.getElementById('status').textContent = [
                owned.supported, owned.smooth, owned.powerEfficient,
                owned.keySystemAccess === null, owned.configuration.video.width,
                videoOnly.supported,
                audioOnly.supported, audioOnly.smooth, unsupportedAudio.supported,
                unsupported.supported, unsupported.smooth,
                encrypted.supported, encrypted.keySystemAccess === null,
                invalid
            ].join(':');
        });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true:true:false:true:320:true:true:true:false:false:false:false:true:TypeError"
    );
}
