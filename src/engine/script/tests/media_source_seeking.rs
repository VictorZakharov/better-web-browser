use super::*;

#[test]
fn unbuffered_mse_seek_waits_for_both_tracks_without_rewinding_to_the_buffer_end() {
    let (dom, mut runtime, _) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const source = new MediaSource();
        source.addEventListener('sourceopen', () => {
            source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
            source.duration = 600;
        });
        movie.src = URL.createObjectURL(source);
        const record = () => document.querySelector('output').textContent =
            [movie.currentTime, movie.seeking, movie.readyState, movie.paused].join(':');
        movie.addEventListener('loadeddata', () => { movie.currentTime = 420; record(); });
        movie.addEventListener('progress', record);
        movie.addEventListener('seeked', record);
    </script>"#,
    );
    let target = dom.elements_named("video").next().unwrap();
    let mut send = |disposition, request_id, time, buffered| {
        runtime
            .dispatch_media_and_tasks(UserInputEvent::Media {
                target: target.clone(),
                request_id,
                disposition,
                current_time: time,
                duration: 600.0,
                width: 320,
                height: 240,
                buffered: Some(buffered),
            })
            .outcome
    };
    let initial = send("loaded", 0, 0.0, [[0.0, 1.0], [0.0, 1.0]]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(
        !initial
            .media_actions
            .iter()
            .any(|a| matches!(a.command, ScriptMediaCommand::Seek { .. })),
        "unbuffered seek was sent to a decoder holding only the first second"
    );
    let output = dom.elements_named("output").next().unwrap();
    assert_eq!(output.text_content(), "420:true:1:true");
    let video = send("appended", 0, 1.0, [[419.0, 421.0], [0.0, 0.0]]);
    assert!(
        !video
            .media_actions
            .iter()
            .any(|a| matches!(a.command, ScriptMediaCommand::Seek { .. }))
    );
    assert_eq!(
        output.text_content(),
        "420:true:1:true",
        "audio still missing"
    );
    send("time", 0, 1.0, [[0.0, 1.0], [0.0, 1.0]]);
    let audio = send("appended", 0, 1.0, [[0.0, 0.0], [419.0, 421.0]]);
    let seek = audio
        .media_actions
        .iter()
        .find(|a| {
            matches!(
                a.command,
                ScriptMediaCommand::Seek {
                    position_100ns: 4_200_000_000
                }
            )
        })
        .expect("seek after both tracks arrive");
    send(
        "seeked",
        seek.request_id,
        420.0,
        [[419.0, 421.0], [419.0, 421.0]],
    );
    assert_eq!(output.text_content(), "420:false:3:true");
}

#[test]
fn newer_seek_ignores_older_completion_and_pause_does_not_resume_playback() {
    let (dom, mut runtime, _) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const source = new MediaSource();
        source.addEventListener('sourceopen', () => {
            source.addSourceBuffer('video/mp4; codecs="avc1.4d401e, mp4a.40.2"');
            source.duration = 600;
        });
        movie.src = URL.createObjectURL(source);
        movie.addEventListener('loadeddata', () => { movie.currentTime = 420; movie.currentTime = 430; });
        movie.addEventListener('timeupdate', () => document.querySelector('output').textContent =
            [movie.currentTime, movie.seeking, movie.paused].join(':'));
    </script>"#,
    );
    let target = dom.elements_named("video").next().unwrap();
    let mut send = |disposition, request_id, time| {
        runtime
            .dispatch_media_and_tasks(UserInputEvent::Media {
                target: target.clone(),
                request_id,
                disposition,
                current_time: time,
                duration: 600.0,
                width: 320,
                height: 240,
                buffered: Some([[0.0, 600.0]; 2]),
            })
            .outcome
    };
    let initial = send("loaded", 0, 0.0);
    let seeks: Vec<_> = initial
        .media_actions
        .iter()
        .filter(|a| matches!(a.command, ScriptMediaCommand::Seek { .. }))
        .collect();
    assert_eq!(seeks.len(), 2);
    let stale = send("seeked", seeks[0].request_id, 420.0);
    assert!(stale.media_actions.is_empty());
    let current = send("seeked", seeks[1].request_id, 430.0);
    assert!(
        current.media_actions.is_empty(),
        "paused seek must not start playback"
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "430:false:true"
    );
}

#[test]
fn pause_cancels_a_play_promise_waiting_on_an_unbuffered_seek() {
    let (dom, mut runtime, _) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const source = new MediaSource();
        source.addEventListener('sourceopen', () => {
            source.addSourceBuffer('video/mp4; codecs="avc1.4d401e, mp4a.40.2"');
            source.duration = 600;
        });
        movie.src = URL.createObjectURL(source);
        movie.addEventListener('loadeddata', () => {
            movie.currentTime = 420;
            movie.play().then(() => document.querySelector('output').textContent = 'unexpected play',
                error => document.querySelector('output').textContent = error.name);
            movie.pause();
        });
    </script>"#,
    );
    let target = dom.elements_named("video").next().unwrap();
    let mut send = |disposition, request_id, buffered| {
        runtime
            .dispatch_media_and_tasks(UserInputEvent::Media {
                target: target.clone(),
                request_id,
                disposition,
                current_time: 420.0,
                duration: 600.0,
                width: 320,
                height: 240,
                buffered: Some(buffered),
            })
            .outcome
    };
    send("loaded", 0, [[0.0, 1.0]; 2]);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "AbortError"
    );
    let append = send("appended", 0, [[419.0, 421.0]; 2]);
    let seek = append
        .media_actions
        .iter()
        .find(|a| matches!(a.command, ScriptMediaCommand::Seek { .. }))
        .unwrap();
    let completed = send("seeked", seek.request_id, [[419.0, 421.0]; 2]);
    assert!(
        completed.media_actions.is_empty(),
        "pause must survive the eventual seek completion"
    );
}
