use super::media_source_segments::execute_media_source;
use super::*;

#[test]
fn media_readiness_tracks_contiguous_buffer_and_queues_transition_events_once() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"
        <video></video><output></output><script>
        const movie = document.querySelector('video'), source = new MediaSource();
        let canplay = 0, through = 0;
        const record = () => document.querySelector('output').textContent =
            [movie.readyState, canplay, through].join(':');
        movie.addEventListener('canplay', () => { canplay++; record(); });
        movie.addEventListener('canplaythrough', () => { through++; record(); });
        movie.addEventListener('timeupdate', record);
        source.addEventListener('sourceopen', () => {
            source.addSourceBuffer('video/mp4; codecs="avc1.4d401e, mp4a.40.2"');
            source.duration = 100;
        });
        movie.src = URL.createObjectURL(source);
        </script>"#,
    );
    assert!(outcome.errors.is_empty());
    for (disposition, current_time, duration, expected) in [
        ("loaded", 0.0, 10.0, "4:1:1"),
        ("time", 1.0, 10.0, "4:1:1"),
        ("time", 8.0, 10.0, "3:1:1"),
        ("time", 10.0, 10.0, "2:1:1"),
        ("appended", 10.0, 20.0, "4:2:2"),
    ] {
        let result = runtime.dispatch_user_input(UserInputEvent::Media {
            target: dom.elements_named("video").next().unwrap(),
            request_id: 0,
            disposition,
            current_time,
            duration,
            width: 320,
            height: 240,
            buffered: None,
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
        assert_eq!(
            dom.elements_named("output").next().unwrap().text_content(),
            expected
        );
    }
}
