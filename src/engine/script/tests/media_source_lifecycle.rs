use super::*;

#[test]
fn load_detaches_the_media_source_and_invalidates_its_buffers() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const source = new MediaSource();
        let buffer, closed = false;
        source.addEventListener('sourceclose', () => {
            closed = true;
            document.querySelector('output').textContent += ':closed-event';
        });
        source.addEventListener('sourceopen', () => {
            buffer = source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            source.duration = 600;
            movie.removeAttribute('src');
            movie.load();
            let failure;
            try { buffer.appendBuffer(new Uint8Array()); } catch (e) { failure = e.name; }
            document.querySelector('output').textContent = [source.readyState,
                source.sourceBuffers.length, source.activeSourceBuffers.length,
                Number.isNaN(source.duration), failure, closed].join(':');
        }, {once: true});
        movie.src = URL.createObjectURL(source);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "closed:0:0:true:InvalidStateError:false:closed-event"
    );
}

#[test]
fn replacing_a_media_source_rejects_old_play_and_preserves_the_new_source() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const old = new MediaSource(), replacement = new MediaSource();
        const log = [];
        old.addEventListener('sourceopen', () => {
            old.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            movie.play().catch(e => log.push(e.name));
            movie.load();
            movie.src = URL.createObjectURL(replacement);
        }, {once: true});
        replacement.addEventListener('sourceopen', () => {
            movie.addEventListener('error', () => document.querySelector('output').textContent = 'selection race');
            replacement.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            replacement.duration = 90;
            queueMicrotask(() => document.querySelector('output').textContent =
                [old.readyState, old.sourceBuffers.length, replacement.readyState,
                 replacement.sourceBuffers.length, movie.duration, ...log].join(':'));
        }, {once: true});
        movie.src = URL.createObjectURL(old);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "closed:0:open:1:90:AbortError"
    );
}

#[test]
fn detached_source_cannot_complete_an_append_into_a_replacement() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const old = new MediaSource(), replacement = new MediaSource();
        old.addEventListener('sourceopen', () => {
            const buffer = old.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            buffer.appendBuffer(new Uint8Array([0,0,0,8,109,100,97,116]));
            buffer.addEventListener('updatestart', () => {
                movie.src = URL.createObjectURL(replacement);
            }, {once: true});
            buffer.addEventListener('updateend', () => {
                document.querySelector('output').textContent =
                    [old.readyState, buffer.updating, replacement.readyState].join(':');
            });
        }, {once: true});
        movie.src = URL.createObjectURL(old);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "closed:false:open"
    );
    assert!(outcome.media_actions.iter().all(|action| !matches!(
        action.command,
        ScriptMediaCommand::Commit { .. } | ScriptMediaCommand::CommitAdaptive { .. }
    )));
}

#[test]
fn load_invalidates_queued_element_events_and_preserves_other_elements() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video id="movie"></video><video id="other"></video><output></output><script>
        const source = new MediaSource(), otherSource = new MediaSource(), log = [];
        other.src = URL.createObjectURL(otherSource);
        source.addEventListener('sourceopen', () => {
            source.duration = 600;
            movie.addEventListener('durationchange', () => log.push('stale-duration'));
            movie.addEventListener('emptied', () => {
                document.querySelector('output').textContent =
                    [source.readyState, otherSource.readyState, movie.currentTime, ...log].join(':');
            });
            movie.removeAttribute('src');
            movie.load();
        }, {once: true});
        movie.src = URL.createObjectURL(source);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "closed:open:0"
    );
}

#[test]
fn load_of_the_same_object_url_reattaches_without_reviving_old_buffers() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const source = new MediaSource();
        let opens = 0, old;
        source.addEventListener('sourceopen', () => {
            if (++opens === 1) {
                old = source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
                movie.load();
            } else {
                let failure;
                try { old.appendBuffer(new Uint8Array()); } catch(e) { failure = e.name; }
                document.querySelector('output').textContent =
                    [opens, source.readyState, source.sourceBuffers.length, failure].join(':');
            }
        });
        movie.setAttributeNS(null, 'src', URL.createObjectURL(source));
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "2:open:0:InvalidStateError"
    );
}

#[test]
fn old_native_load_reply_cannot_restore_a_detached_resource() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const old = new MediaSource(), replacement = new MediaSource();
        let buffer;
        old.addEventListener('sourceopen', () => {
            buffer = old.addSourceBuffer('video/mp4; codecs="avc1.4d401e, mp4a.40.2"');
            buffer.appendBuffer(new Uint8Array([0,0,0,8,109,100,97,116]));
        }, {once: true});
        movie.addEventListener('click', () => { movie.src = URL.createObjectURL(replacement); });
        movie.addEventListener('loadeddata', () => document.querySelector('output').textContent = 'stale');
        movie.addEventListener('timeupdate', () => document.querySelector('output').textContent = 'stale-clock');
        replacement.addEventListener('sourceopen', () => {
            replacement.duration = 90;
            document.querySelector('output').textContent = String(movie.duration);
        }, {once: true});
        movie.src = URL.createObjectURL(old);
        </script>"#,
    );
    let commit = outcome
        .media_actions
        .iter()
        .find(|action| matches!(action.command, ScriptMediaCommand::Commit { .. }))
        .expect("pending old commit");
    assert_ne!(commit.request_id, 0);
    let target = dom.elements_named("video").next().unwrap();
    runtime.dispatch_user_input(UserInputEvent::Simple {
        target: target.clone(),
        event_type: "click",
        bubbles: true,
        cancelable: true,
    });
    runtime.advance_time(Duration::ZERO, 32);
    let result = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
        target: target.clone(),
        request_id: commit.request_id,
        disposition: "loaded",
        current_time: 0.0,
        duration: 600.0,
        width: 320,
        height: 240,
        buffered: Some([[0.0, 600.0]; 2]),
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    let clock = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
        target,
        request_id: 0,
        disposition: "time",
        current_time: 600.0,
        duration: 600.0,
        width: 320,
        height: 240,
        buffered: Some([[0.0, 600.0]; 2]),
    });
    assert!(
        clock.outcome.errors.is_empty(),
        "{:?}",
        clock.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "90"
    );
}
