use super::media_source_segments::execute_media_source;
use super::*;

#[test]
fn media_source_updateend_observes_worker_accepted_track_ranges() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"
        <video></video><output>pending</output><script>
        const source = new MediaSource(), movie = document.querySelector('video');
        const records = [];
        source.addEventListener('sourceopen', () => {
            for (const type of ['video/mp4; codecs="avc1.4d401e"', 'audio/mp4; codecs="mp4a.40.2"']) {
                const buffer = source.addSourceBuffer(type);
                buffer.addEventListener('updateend', () => {
                    records.push([buffer.updating, buffer.buffered.end(0), movie.buffered.end(0)].join(':'));
                    document.querySelector('output').textContent = records.join('|');
                });
                buffer.appendBuffer(new Uint8Array([0,0,0,9,109,100,97,116,1]));
            }
        });
        movie.src = URL.createObjectURL(source);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "pending"
    );
    let response = runtime.dispatch_user_input(UserInputEvent::Media {
        target: dom.elements_named("video").next().unwrap(),
        request_id: 0,
        disposition: "loaded",
        current_time: 0.0,
        duration: 8.0,
        width: 320,
        height: 240,
        buffered: Some([[0.0, 5.0], [0.0, 8.0]]),
    });
    assert!(
        response.outcome.errors.is_empty(),
        "{:?}",
        response.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false:5:5|false:8:5"
    );
}

#[test]
fn media_source_intersects_track_ranges_without_fabricating_or_filling_gaps() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"
        <video id="movie"></video><output></output><script>
        const movie = document.getElementById('movie'), source = new MediaSource();
        source.addEventListener('sourceopen', () => {
            source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
            source.duration = 220;
        });
        const record = () => {
            const ranges = value => Array.from({length:value.length}, (_, i) =>
                value.start(i) + '-' + value.end(i)).join(',');
            document.querySelector('output').textContent =
                [...source.sourceBuffers].map(buffer => ranges(buffer.buffered)).join('|')
                + '|' + ranges(movie.buffered);
        };
        movie.addEventListener('loadedmetadata', record);
        movie.addEventListener('progress', record);
        movie.src = URL.createObjectURL(source);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    for (disposition, buffered, expected) in [
        ("loaded", [[0.0, 10.0], [0.0, 20.0]], "0-10|0-20|0-10"),
        ("appended", [[10.0, 30.0], [0.0, 0.0]], "0-30|0-20|0-20"),
        ("appended", [[0.0, 0.0], [20.0, 40.0]], "0-30|0-40|0-30"),
        (
            "appended",
            [[50.0, 60.0], [50.0, 70.0]],
            "0-30,50-60|0-40,50-70|0-30,50-60",
        ),
    ] {
        let result = runtime.dispatch_user_input(UserInputEvent::Media {
            target: dom.elements_named("video").next().unwrap(),
            request_id: 0,
            disposition,
            current_time: 0.0,
            duration: f64::max(buffered[0][1], buffered[1][1]),
            width: 320,
            height: 240,
            buffered: Some(buffered),
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

#[test]
fn media_source_can_append_one_track_without_waiting_for_the_other() {
    for track in ["video", "audio"] {
        let script = format!(
            r#"<video></video><script>
            const source = new MediaSource(), movie = document.querySelector('video');
            const bytes = new Uint8Array([0,0,0,8,109,111,111,102,0,0,0,9,109,100,97,116,1]);
            let video, audio;
            source.addEventListener('sourceopen', () => {{
                video = source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
                audio = source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
            }});
            movie.addEventListener('loadedmetadata', () => {track}.appendBuffer(bytes));
            movie.src = URL.createObjectURL(source);
            </script>"#
        );
        let (dom, mut runtime, outcome) = execute_media_source(&script);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let result = runtime.dispatch_user_input(UserInputEvent::Media {
            target: dom.elements_named("video").next().unwrap(),
            request_id: 0,
            disposition: "loaded",
            current_time: 0.0,
            duration: 10.0,
            width: 320,
            height: 240,
            buffered: Some([[0.0, 10.0], [0.0, 10.0]]),
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
        let (video, audio) = result
            .outcome
            .media_actions
            .iter()
            .find_map(|action| match &action.command {
                ScriptMediaCommand::AppendAdaptive {
                    video_bytes,
                    audio_bytes,
                } => Some((video_bytes, audio_bytes)),
                _ => None,
            })
            .expect("ready single-track data must not wait for another track");
        assert_eq!(video.is_empty(), track != "video");
        assert_eq!(audio.is_empty(), track != "audio");
    }
}
