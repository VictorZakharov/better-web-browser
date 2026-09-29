use super::*;

#[test]
fn src_object_detaches_mse_and_ignores_late_worker_replies() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const source = new MediaSource(), stream = new MediaStream([]);
        movie.addEventListener('click', () => {
            document.querySelector('output').textContent = [
                source.readyState, source.sourceBuffers.length,
                movie.srcObject === stream, movie.currentSrc, movie.duration,
                movie.readyState, movie.currentTime, movie.paused,
                movie.buffered.length, movie.videoWidth, movie.networkState
            ].join(':');
        });
        source.addEventListener('sourceopen', () => {
            source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            source.duration = 90;
            movie.srcObject = stream;
        }, {once: true});
        movie.src = URL.createObjectURL(source);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome
            .media_actions
            .iter()
            .any(|action| matches!(action.command, ScriptMediaCommand::Reset))
    );
    let target = dom.elements_named("video").next().unwrap();
    for disposition in ["loaded", "playing", "time"] {
        let result = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
            target: target.clone(),
            request_id: 0,
            disposition,
            current_time: 12.0,
            duration: 90.0,
            width: 320,
            height: 240,
            buffered: Some([[0.0, 90.0]; 2]),
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
    }
    let result = runtime.dispatch_user_input(UserInputEvent::Simple {
        target,
        event_type: "click",
        bubbles: true,
        cancelable: true,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "closed:0:true::Infinity:0:0:true:0:0:1"
    );
}

#[test]
fn src_object_reassignment_resets_and_null_restores_url_selection() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const stream = new MediaStream([]);
        movie.src = '/fallback.mp4';
        movie.srcObject = stream;
        movie.play();
        movie.currentTime = 7;
        movie.srcObject = stream;
        document.body.setAttribute('data-repeat', [
            movie.srcObject === stream, movie.paused, movie.currentTime,
            movie.networkState, movie.readyState
        ].join(':'));
        movie.play();
        movie.load();
        document.body.setAttribute('data-load', [
            movie.srcObject === stream, movie.paused, movie.currentTime,
            movie.networkState
        ].join(':'));
        movie.src = '/replacement.mp4';
        document.body.setAttribute('data-attribute', [
            movie.srcObject === stream, movie.currentSrc, movie.networkState
        ].join(':'));
        movie.addEventListener('click', () => {
            if (!movie.srcObject) {
                document.querySelector('output').textContent = [
                    movie.currentSrc, movie.readyState, movie.duration, movie.videoWidth
                ].join(':');
                return;
            }
            document.body.setAttribute('data-before-null', [
                movie.currentSrc, movie.error === null, movie.networkState,
                movie.duration, movie.paused
            ].join(':'));
            movie.srcObject = null;
        });
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-repeat")
            .as_deref(),
        Some("true:true:0:1:0")
    );
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-load").as_deref(), Some("true:true:0:1"));
    assert_eq!(body.attr("data-attribute").as_deref(), Some("true::1"));
    assert_eq!(
        outcome
            .media_actions
            .iter()
            .filter(|action| matches!(action.command, ScriptMediaCommand::Reset))
            .count(),
        4
    );
    let target = dom.elements_named("video").next().unwrap();
    for disposition in ["selected", "error"] {
        let result = runtime.dispatch_user_input(UserInputEvent::MediaSource {
            target: target.clone(),
            disposition,
            source_url: "https://example.com/stale.mp4".into(),
            reason: "network",
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
    }
    let result = runtime.dispatch_user_input(UserInputEvent::Simple {
        target: target.clone(),
        event_type: "click",
        bubbles: true,
        cancelable: true,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-before-null")
            .as_deref(),
        Some(":true:1:Infinity:true")
    );
    let reset_request_id = result
        .outcome
        .media_actions
        .iter()
        .find(|action| matches!(action.command, ScriptMediaCommand::Reset))
        .expect("capture-to-URL reset")
        .request_id;
    let stale = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
        target: target.clone(),
        request_id: 0,
        disposition: "loaded",
        current_time: 12.0,
        duration: 90.0,
        width: 320,
        height: 240,
        buffered: Some([[0.0, 90.0]; 2]),
    });
    assert!(
        stale.outcome.errors.is_empty(),
        "{:?}",
        stale.outcome.errors
    );
    let inspect = |runtime: &mut ScriptRuntime| {
        runtime.dispatch_user_input(UserInputEvent::Simple {
            target: target.clone(),
            event_type: "click",
            bubbles: true,
            cancelable: true,
        })
    };
    let before_reset = inspect(&mut runtime);
    assert!(
        before_reset.outcome.errors.is_empty(),
        "{:?}",
        before_reset.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        ":0:NaN:0"
    );
    let reset = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
        target: target.clone(),
        request_id: reset_request_id,
        disposition: "reset",
        current_time: 0.0,
        duration: f64::NAN,
        width: 0,
        height: 0,
        buffered: None,
    });
    assert!(
        reset.outcome.errors.is_empty(),
        "{:?}",
        reset.outcome.errors
    );
    let selected = runtime.dispatch_user_input(UserInputEvent::MediaSource {
        target: target.clone(),
        disposition: "selected",
        source_url: "https://example.com/replacement.mp4".into(),
        reason: "",
    });
    assert!(
        selected.outcome.errors.is_empty(),
        "{:?}",
        selected.outcome.errors
    );
    let fresh = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
        target: target.clone(),
        request_id: 0,
        disposition: "loaded",
        current_time: 0.0,
        duration: 90.0,
        width: 320,
        height: 240,
        buffered: Some([[0.0, 90.0]; 2]),
    });
    assert!(
        fresh.outcome.errors.is_empty(),
        "{:?}",
        fresh.outcome.errors
    );
    let after_reset = inspect(&mut runtime);
    assert!(
        after_reset.outcome.errors.is_empty(),
        "{:?}",
        after_reset.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "https://example.com/replacement.mp4:4:90:320"
    );
    let followup = runtime.advance_time(Duration::ZERO, 32);
    assert!(followup.errors.is_empty(), "{:?}", followup.errors);
    assert!(
        result
            .outcome
            .media_actions
            .iter()
            .chain(stale.outcome.media_actions.iter())
            .chain(followup.media_actions.iter())
            .any(|action| matches!(action.command, ScriptMediaCommand::Reload))
    );
}
