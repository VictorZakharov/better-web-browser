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
                video.canPlayType(source.type) === '',
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
    let (dom, outcome) = execute_html(
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
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "NotSupportedError,IndexSizeError,IndexSizeError,true"
    );
}
