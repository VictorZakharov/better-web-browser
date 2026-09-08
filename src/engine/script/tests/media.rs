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
        ]).then(([owned, unsupported, encrypted, invalid]) => {
            document.getElementById('status').textContent = [
                owned.supported, owned.smooth, owned.powerEfficient,
                owned.keySystemAccess === null, owned.configuration.video.width,
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
        "true:true:false:true:320:false:false:false:true:TypeError"
    );
}
