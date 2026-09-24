use super::*;

#[test]
fn ready_is_stable_during_pending_play_and_pause_and_resolves_after_a_checkpoint() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const animation = new Animation(new KeyframeEffect(target,
            [{opacity: 0}, {opacity: 1}], {duration: 10000, fill: 'both'}));
        const initial = animation.ready;
        animation.play();
        const playing = animation.ready;
        const checks = [initial !== playing, playing === animation.ready,
            animation.pending, animation.startTime === null, animation.playState === 'running'];
        animation.pause();
        checks.push(animation.pending, animation.ready === playing,
            animation.playState === 'paused');
        playing.then(value => {
            checks.push(value === animation, !animation.pending,
                animation.ready === playing, animation.currentTime === 0);
            animation.cancel();
            document.body.setAttribute('data-result', checks.join(':'));
        });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true:true:true:true:true:true:true:true:true:true:true")
    );
}

#[test]
fn canceling_pending_play_rejects_old_ready_and_replaces_it_with_resolved_promise() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const animation = document.getElementById('target').animate(
            [{opacity: 0}, {opacity: 1}], {duration: 1000, fill: 'both'});
        const pending = animation.ready;
        const checks = [animation.pending];
        pending.then(() => checks.push(false), error => {
            checks.push(error.name === 'AbortError', animation.playState === 'idle',
                animation.ready !== pending);
            animation.ready.then(value => {
                checks.push(value === animation);
                document.body.setAttribute('data-result', checks.join(':'));
            });
        });
        animation.cancel();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true:true:true:true")
    );
}

#[test]
fn finished_promise_identity_survives_finish_and_is_replaced_only_on_replay() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const animation = document.getElementById('target').animate(
            [{opacity: 0}, {opacity: 1}], {duration: 1000, fill: 'both'});
        const first = animation.finished;
        const checks = [first === animation.finished];
        animation.finish();
        checks.push(animation.finished === first, animation.playState === 'finished');
        first.then(value => {
            checks.push(value === animation);
            animation.play();
            checks.push(animation.finished !== first, animation.pending);
            animation.finish();
            checks.push(animation.playState === 'finished');
            document.body.setAttribute('data-result', checks.join(':'));
        });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true:true:true:true:true:true")
    );
}

#[test]
fn playback_events_expose_read_only_times_and_reverse_infinite_animation_fails() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const event = new AnimationPlaybackEvent('finish',
            {currentTime: 100, timelineTime: 200});
        const descriptor = Object.getOwnPropertyDescriptor(event, 'currentTime');
        const animation = new Animation(new KeyframeEffect(
            document.getElementById('target'), [{opacity: 0}, {opacity: 1}],
            {duration: 100, iterations: Infinity}));
        let rejected = false;
        animation.playbackRate = -1;
        try { animation.play(); } catch (error) {
            rejected = error.name === 'InvalidStateError';
        }
        document.body.setAttribute('data-result', [event.currentTime === 100,
            event.timelineTime === 200, descriptor.writable === false,
            rejected, animation.playState === 'idle'].join(':'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true:true:true:true")
    );
}

#[test]
fn repeated_play_keeps_ready_identity_and_setting_start_time_completes_pending_play() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const animation = document.getElementById('target').animate(
            [{opacity: 0}, {opacity: 1}], {duration: 1000, fill: 'both'});
        const pending = animation.ready;
        animation.play();
        const checks = [animation.ready === pending, animation.pending,
            animation.startTime === null];
        animation.startTime = document.timeline.currentTime - 100;
        checks.push(!animation.pending, animation.ready === pending,
            animation.startTime !== null, animation.currentTime >= 100);
        pending.then(value => {
            checks.push(value === animation);
            animation.cancel();
            checks.push(animation.ready !== pending, animation.playState === 'idle');
            document.body.setAttribute('data-result', checks.join(':'));
        });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true:true:true:true:true:true:true:true:true")
    );
}
