use super::*;

#[test]
fn ended_tracks_extend_the_last_range_but_never_fill_an_internal_gap() {
    let (dom, mut runtime, _) = execute_media_source(
        r#"
        <video id="movie"></video><output></output><script>
        const source = new MediaSource();
        source.addEventListener('sourceopen', () => {
            source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
            source.duration = 5;
        });
        movie.src = URL.createObjectURL(source);
        movie.addEventListener('progress', () => {
            source.endOfStream();
            document.querySelector('output').textContent = [movie.duration, movie.buffered.length,
                movie.buffered.start(0), movie.buffered.end(0),
                movie.buffered.start(1), movie.buffered.end(1)].join(':');
            source.sourceBuffers[0].appendBuffer(new Uint8Array());
            document.querySelector('output').textContent += ':' + movie.buffered.end(1);
        });
    </script>"#,
    );
    let target = dom.elements_named("video").next().unwrap();
    for (disposition, buffered) in [
        ("loaded", [[0.0, 1.0], [0.0, 5.0]]),
        ("appended", [[3.0, 4.0], [0.0, 0.0]]),
    ] {
        let outcome = runtime
            .dispatch_user_input(UserInputEvent::Media {
                target: target.clone(),
                request_id: 0,
                disposition,
                current_time: 0.0,
                duration: 5.0,
                width: 320,
                height: 240,
                buffered: Some(buffered),
            })
            .outcome;
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "5:2:0:1:3:5:4"
    );
}

#[test]
fn shorter_video_buffer_stops_audio_and_waits_for_both_tracks() {
    for pause_while_waiting in [false, true] {
        let html = format!(
            r#"<video id="movie"></video><output></output><span></span><script>
            const source = new MediaSource();
            source.addEventListener('sourceopen', () => {{
                source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
                source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
                source.duration = 600;
            }});
            movie.src = URL.createObjectURL(source);
            const events = [];
            for (const name of ['waiting', 'pause', 'seeking', 'seeked', 'ended'])
                movie.addEventListener(name, () => {{
                    events.push(name);
                    document.querySelector('output').textContent = events.join(',');
                    if (name === 'waiting' && {pause_while_waiting}) movie.pause();
                }});
            movie.addEventListener('progress', () => document.querySelector('span').textContent =
                [movie.currentTime, movie.readyState, movie.seeking, movie.paused].join(':'));
        </script>"#
        );
        let (dom, mut runtime, outcome) = execute_media_source(&html);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let target = dom.elements_named("video").next().unwrap();
        let mut send = |disposition, request_id, current_time, buffered| {
            let outcome = runtime
                .dispatch_user_input(UserInputEvent::Media {
                    target: target.clone(),
                    request_id,
                    disposition,
                    current_time,
                    duration: 292.0,
                    width: 320,
                    height: 240,
                    buffered: Some(buffered),
                })
                .outcome;
            assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
            outcome
        };
        let initial = [[270.0, 272.0], [270.0, 292.0]];
        send("loaded", 0, 270.0, initial);
        send("playing", 0, 270.0, initial);
        let stalled = send("time", 0, 272.2, initial);
        let pause = stalled
            .media_actions
            .iter()
            .find(|action| {
                matches!(
                    action.command,
                    ScriptMediaCommand::SetPlayback { playing: false, .. }
                ) && action.request_id != 0
            })
            .expect("audio must stop at the video buffer end, not twenty seconds later");
        send("paused", pause.request_id, 272.3, initial);
        // A late native clock response cannot move the held timeline forward.
        send("time", 0, 273.0, initial);
        let audio = send("appended", 0, 273.0, [[0.0, 0.0], [292.0, 300.0]]);
        assert!(
            audio.media_actions.is_empty(),
            "audio-only append resumed missing video"
        );
        assert_eq!(
            dom.elements_named("span").next().unwrap().text_content(),
            format!("272:2:false:{pause_while_waiting}")
        );
        let video = send("appended", 0, 273.0, [[272.0, 276.0], [0.0, 0.0]]);
        let seek = video
            .media_actions
            .iter()
            .find(|action| {
                matches!(
                    action.command,
                    ScriptMediaCommand::Seek {
                        position_100ns: 2_720_000_000
                    }
                )
            })
            .expect("restore the held clock before resuming");
        let resumed = send(
            "seeked",
            seek.request_id,
            272.0,
            [[270.0, 276.0], [270.0, 300.0]],
        );
        assert_eq!(
            resumed.media_actions.iter().any(|action| matches!(
                action.command,
                ScriptMediaCommand::SetPlayback { playing: true, .. }
            )),
            !pause_while_waiting
        );
        assert_eq!(
            dom.elements_named("output").next().unwrap().text_content(),
            if pause_while_waiting {
                "waiting,pause"
            } else {
                "waiting"
            },
            "internal buffering must not expose a user seek, pause, or ended event"
        );
    }
}
