use super::*;

#[test]
fn serialized_keyframes_use_idl_property_names_and_discard_invalid_values() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target, [
            {backgroundColor: 'red', opacity: 'not-a-number', left: '0px'},
            {backgroundColor: 'blue', opacity: 1, left: '60px'}
        ], 1000);
        const frames = effect.getKeyframes();
        const result = [
            frames[0].backgroundColor === 'red',
            !Object.hasOwn(frames[0], 'background-color'),
            !Object.hasOwn(frames[0], 'opacity'),
            frames[1].opacity === '1',
            frames[1].left === '60px'
        ];
        document.body.setAttribute('data-result', result.join(':'));
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
fn property_indexed_frames_keep_sparse_offsets_after_invalid_values_are_removed() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const effect = new KeyframeEffect(document.getElementById('target'), {
            opacity: [0, 'invalid', 1],
            left: ['0px', '20px']
        }, {duration: 100, fill: 'both'});
        const frames = effect.getKeyframes();
        document.body.setAttribute('data-result', frames.map(frame =>
            `${frame.computedOffset}:${frame.opacity ?? '-'}:${frame.left ?? '-'}`).join('|'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("0:0:0px|1:1:20px")
    );
}

#[test]
fn step_easing_respects_start_end_none_and_both_boundaries() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const sample = (easing, time) => {
            const effect = new KeyframeEffect(target, [{opacity: 0}, {opacity: 1}],
                {duration: 100, fill: 'both', easing});
            const animation = new Animation(effect);
            animation.currentTime = time;
            return effect.getComputedTiming().progress;
        };
        const checks = [
            sample('step-start', 0) === 1,
            sample('step-end', 0) === 0,
            sample('steps(4, jump-start)', 0) === .25,
            sample('steps(4, jump-start)', 25) === .5,
            sample('steps(4, jump-end)', 25) === .25,
            sample('steps(3, jump-none)', 0) === 0,
            sample('steps(3, jump-none)', 50) === .5,
            sample('steps(3, jump-both)', 0) === .25,
            sample('steps(3, jump-both)', 50) === .5,
            sample('steps(3, jump-both)', 100) === 1
        ];
        document.body.setAttribute('data-result', checks.join(':'));
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

#[test]
fn impossible_jump_none_and_invalid_css_values_are_rejected_without_mutating_effect() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target, [{opacity: 0}, {opacity: 1}], 100);
        const before = JSON.stringify(effect.getKeyframes());
        let rejected = false;
        try { effect.setKeyframes([{opacity: 0, easing: 'steps(1, jump-none)'}]); }
        catch (error) { rejected = error instanceof TypeError; }
        document.body.setAttribute('data-result',
            [rejected, JSON.stringify(effect.getKeyframes()) === before].join(':'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true")
    );
}

#[test]
fn custom_document_timeline_controls_animation_start_time_and_retargeting_preserves_time() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const early = new DocumentTimeline({originTime: -5000});
        const later = new DocumentTimeline({originTime: -10000});
        const animation = new Animation(new KeyframeEffect(target,
            [{opacity: 0}, {opacity: 1}], {duration: 100000, fill: 'both'}), early);
        animation.play();
        animation.currentTime = 250;
        const wasPending = animation.pending && animation.startTime === null;
        animation.ready.then(() => {
            const current = animation.currentTime;
            const start = animation.startTime;
            animation.timeline = later;
            const result = [wasPending, Math.abs(current - 250) < 20,
                start > 4000 && start < 6000,
                Math.abs(animation.currentTime - current) < 20,
                animation.startTime > start + 4000,
                animation.timeline === later];
            animation.cancel();
            document.body.setAttribute('data-result', result.join(':'));
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
        Some("true:true:true:true:true:true")
    );
}

#[test]
fn backwards_fill_remains_in_effect_after_reverse_finish() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity:.4'></div><script>
        const target = document.getElementById('target');
        const animation = target.animate([{opacity: 0}, {opacity: 1}],
            {delay: 10, duration: 100, fill: 'backwards'});
        animation.pause();
        animation.currentTime = 80;
        animation.playbackRate = -1;
        animation.finish();
        const result = [animation.playState, animation.currentTime,
            getComputedStyle(target).opacity, target.getAnimations().includes(animation)].join(':');
        animation.cancel();
        document.body.setAttribute('data-result', result);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("finished:0:0:true")
    );
}

#[test]
fn cloning_effect_copies_keyframes_and_timing_without_sharing_mutable_state() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><div id=other></div><script>
        const target = document.getElementById('target');
        const source = new KeyframeEffect(target,
            [{opacity: 0, easing: 'ease-in'}, {opacity: 1}],
            {duration: 1000, iterations: 2, fill: 'both'});
        const clone = new KeyframeEffect(source);
        source.target = document.getElementById('other');
        source.setKeyframes([{opacity: .2}, {opacity: .8}]);
        source.updateTiming({duration: 200});
        const frames = clone.getKeyframes(), timing = clone.getTiming();
        document.body.setAttribute('data-result', [
            clone.target === target,
            frames.length === 2 && frames[0].opacity === '0' && frames[1].opacity === '1',
            frames[0].easing === 'ease-in',
            timing.duration === 1000 && timing.iterations === 2 && timing.fill === 'both',
            clone.pseudoElement === null && clone.composite === 'replace'
        ].join(':'));
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
fn pseudo_element_and_additive_effect_options_fail_explicitly() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const unsupported = callback => { try { callback(); return false; }
            catch (error) { return error.name === 'NotSupportedError'; } };
        const effect = new KeyframeEffect(target, [{opacity: 0}, {opacity: 1}], 100);
        document.body.setAttribute('data-result', [
            unsupported(() => new KeyframeEffect(target, [{opacity: 1}],
                {pseudoElement: '::before'})),
            unsupported(() => new KeyframeEffect(target, [{opacity: 1}],
                {composite: 'add'})),
            unsupported(() => { effect.pseudoElement = '::after'; }),
            effect.pseudoElement === null && effect.target === target
        ].join(':'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true:true:true")
    );
}
