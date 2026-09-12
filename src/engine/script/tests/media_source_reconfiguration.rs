use super::*;

#[test]
fn later_media_only_appends_use_the_latest_initialization_segment() {
    for split_init in [false, true] {
        let html = format!(
            r#"<video id="movie"></video><script>
            const source = new MediaSource();
            const box = (name, value) => new Uint8Array([
                0,0,0,9, ...Array.from(name, c => c.charCodeAt(0)), value]);
            const join = (...parts) => new Uint8Array(parts.flatMap(p => Array.from(p)));
            source.addEventListener('sourceopen', () => {{
                const video = source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
                const audio = source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
                const init = value => join(box('ftyp',0), box('moov',value));
                const media = value => join(box('moof',0), box('mdat',value));
                const segments = {split_init}
                    ? [join(init(1),media(11)),init(2),media(22),media(33)]
                    : [join(init(1),media(11)),join(init(2),media(22)),media(33)];
                let index = 0;
                video.addEventListener('updateend', () => {{
                    if (++index < segments.length) video.appendBuffer(segments[index]);
                }});
                video.appendBuffer(segments[0]);
                audio.appendBuffer(join(init(9),media(99)));
            }});
            movie.src = URL.createObjectURL(source);
        </script>"#
        );
        let (dom, mut runtime, mut outcome) = execute_media_source(&html);
        for (index, expected) in [(0, (1, 11)), (1, (2, 22)), (2, (2, 33))] {
            assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
            let bytes = outcome
                .media_actions
                .iter()
                .find_map(|action| match &action.command {
                    ScriptMediaCommand::CommitAdaptive { video_bytes, .. }
                    | ScriptMediaCommand::AppendAdaptive { video_bytes, .. } => Some(video_bytes),
                    _ => None,
                })
                .expect("expected a media transfer");
            assert_eq!(
                (bytes[17], *bytes.last().unwrap()),
                expected,
                "transfer {index}, split initialization={split_init}"
            );
            outcome = runtime
                .dispatch_media_and_tasks(UserInputEvent::Media {
                    target: dom.elements_named("video").next().unwrap(),
                    request_id: 0,
                    disposition: if index == 0 { "loaded" } else { "appended" },
                    current_time: 0.0,
                    duration: (index + 1) as f64,
                    width: 320,
                    height: 240,
                    buffered: None,
                })
                .outcome;
        }
    }
}

#[test]
fn decode_failure_reports_one_media_error_and_keeps_document_script_alive() {
    for mse in [false, true] {
        let html = format!(
            r#"<video id="movie"></video><output></output><script>
            const movie = document.querySelector('video');
            if ({mse}) movie.src = URL.createObjectURL(new MediaSource());
            let errors = 0;
            movie.addEventListener('error', () => {{
                document.querySelector('output').textContent =
                    [++errors, movie.error.code, movie.networkState, movie.videoWidth].join(':');
            }});
        </script>"#
        );
        let (dom, mut runtime, _) = execute_media_source(&html);
        for disposition in ["loaded", "media-error", "media-error"] {
            let result = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
                target: dom.elements_named("video").next().unwrap(),
                request_id: 0,
                disposition,
                current_time: 0.5,
                duration: 1.0,
                width: 320,
                height: 240,
                buffered: None,
            });
            assert!(
                result.outcome.errors.is_empty(),
                "{:?}",
                result.outcome.errors
            );
        }
        assert_eq!(
            dom.elements_named("output").next().unwrap().text_content(),
            "1:3:1:320"
        );
    }
}
