use super::*;

#[test]
fn element_animations_interpolate_without_mutating_inline_style() {
    let (dom, outcome) = execute_html(
        r#"<style>#target { opacity: .2; color: blue }</style>
        <body><div id=target style='opacity: .1'></div><script>
        const failures = [];
        const check = (name, value) => { if (!value) failures.push(name); };
        const target = document.getElementById('target');
        const animation = target.animate([{opacity: 0}, {opacity: 1}],
            {duration: 1000, fill: 'both'});
        animation.pause();
        animation.currentTime = 500;
        check('constructor', animation instanceof Animation &&
            animation.effect instanceof KeyframeEffect && document.timeline instanceof DocumentTimeline);
        check('interpolation', getComputedStyle(target).opacity === '0.5');
        check('inline style', target.getAttribute('style') === 'opacity: .1' &&
            target.style.opacity === '.1');
        check('timing', animation.effect.getComputedTiming().progress === 0.5 &&
            animation.effect.getKeyframes().length === 2);
        check('enumeration', target.getAnimations().includes(animation) &&
            document.getAnimations().includes(animation));
        animation.cancel();
        check('cancel', getComputedStyle(target).opacity === '0.1' &&
            target.getAnimations().length === 0);
        document.body.setAttribute('data-result', failures.join(',') || 'pass');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("pass")
    );
}

#[test]
fn important_author_declarations_remain_above_animation_origin() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity: .3 !important'></div><script>
        const target = document.getElementById('target');
        const animation = target.animate([{opacity: 0}, {opacity: 1}],
            {duration: 1000, fill: 'both'});
        animation.pause();
        animation.currentTime = 500;
        const before = getComputedStyle(target).opacity;
        target.style.removeProperty('opacity');
        const after = getComputedStyle(target).opacity;
        animation.cancel();
        document.body.setAttribute('data-result', before + ':' + after);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("0.3:0.5")
    );
}

#[test]
fn keyframe_offsets_and_invalid_timing_follow_the_web_animations_contract() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        const target = document.createElement('div');
        const effect = new KeyframeEffect(target,
            [{opacity: 0}, {opacity: .25}, {opacity: 1}],
            {duration: 1000, direction: 'alternate', iterations: 2});
        let invalidOffset = false, invalidDuration = false;
        try { effect.setKeyframes([{offset: .8}, {offset: .2}]); }
        catch (error) { invalidOffset = error instanceof TypeError; }
        try { effect.updateTiming({duration: -1}); }
        catch (error) { invalidDuration = error instanceof TypeError; }
        document.body.setAttribute('data-result', [
            effect.getKeyframes().map(frame => frame.computedOffset).join(','),
            effect.getComputedTiming().activeDuration,
            invalidOffset, invalidDuration
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
        Some("0,0.5,1:2000:true:true")
    );
}

#[test]
fn translated_keyframes_interpolate_and_return_to_the_underlying_transform() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='transform:translateX(10px)'></div><script>
        const target = document.getElementById('target');
        const animation = target.animate([
            {transform: 'translateX(0px)'}, {transform: 'translateX(100px)'}
        ], {duration: 1000, fill: 'both'});
        animation.pause(); animation.currentTime = 500;
        const middle = getComputedStyle(target).transform;
        animation.cancel();
        const restored = getComputedStyle(target).transform;
        document.body.setAttribute('data-result', middle + ':' + restored);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("translate(50px, 0px):translate(10px, 0px)")
    );
}

#[test]
fn animation_finishes_on_render_frames_and_dispatches_playback_event() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const animation = target.animate([{opacity: 0}, {opacity: 1}],
            {duration: 48, fill: 'forwards'});
        // Move the running clock to the boundary; the next rendering step must
        // settle it without depending on real wall-clock time in this test.
        animation.currentTime = 48;
        animation.addEventListener('finish', event => {
            document.body.setAttribute('data-result', [
                animation.playState, event instanceof AnimationPlaybackEvent,
                event.currentTime >= 48, getComputedStyle(target).opacity,
                document.getAnimations().includes(animation)
            ].join(':'));
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
        Some("finished:true:true:1:true")
    );
}

#[test]
fn changing_effect_target_moves_the_animation_origin() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=first style='opacity:.2'></div>
        <div id=second style='opacity:.3'></div><script>
        const first = document.getElementById('first');
        const second = document.getElementById('second');
        const animation = first.animate([{opacity: 0}, {opacity: 1}],
            {duration: 1000, fill: 'both'});
        animation.pause(); animation.currentTime = 500;
        animation.effect.target = second;
        const values = [getComputedStyle(first).opacity, getComputedStyle(second).opacity];
        animation.cancel();
        document.body.setAttribute('data-result', values.join(':'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("0.2:0.5")
    );
}

#[test]
fn iteration_boundaries_are_exclusive_except_when_forwards_filling() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const animation = target.animate([{opacity: 0}, {opacity: 1}],
            {duration: 100, iterations: 2, fill: 'forwards'});
        animation.pause();
        animation.currentTime = 100;
        const boundary = animation.effect.getComputedTiming();
        const atBoundary = getComputedStyle(target).opacity;
        animation.currentTime = 150;
        const halfway = getComputedStyle(target).opacity;
        animation.currentTime = 200;
        const filled = animation.effect.getComputedTiming();
        document.body.setAttribute('data-result', [boundary.progress,
            boundary.currentIteration, atBoundary, halfway, filled.progress,
            filled.currentIteration, getComputedStyle(target).opacity].join(':'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("0:1:0:0.5:1:1:1")
    );
}

#[test]
fn computed_timing_preserves_auto_and_accepts_infinite_iteration_values() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target, [{opacity: 0}, {opacity: 1}]);
        const initial = effect.getTiming().duration === 'auto' &&
            effect.getComputedTiming().duration === 0 &&
            effect.getComputedTiming().fill === 'none';
        effect.updateTiming({duration: 100, iterations: Infinity,
            iterationStart: 0.25, direction: 'alternate'});
        const animation = new Animation(effect);
        animation.play(); animation.pause(); animation.currentTime = 125;
        const timing = effect.getComputedTiming();
        const repeating = timing.activeDuration === Infinity &&
            timing.currentIteration === 1 && timing.progress === 0.5 &&
            getComputedStyle(target).opacity === '0.5';
        effect.updateTiming({duration: 0, iterations: Infinity});
        const zero = effect.getComputedTiming().activeDuration === 0;
        document.body.setAttribute('data-result', [initial, repeating, zero].join(':'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true:true")
    );
}

#[test]
fn property_indexed_keyframes_keep_independent_offsets_and_specified_nulls() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target, {
            opacity: [0, 1], left: ['0px', '30px', '60px', '90px']
        }, {duration: 1000, fill: 'both'});
        const frames = effect.getKeyframes();
        const offsets = frames.map(frame => frame.computedOffset).join(',');
        const specified = frames.every(frame => frame.offset === null);
        const animation = new Animation(effect);
        animation.play(); animation.pause(); animation.currentTime = 500;
        const result = [offsets, specified, getComputedStyle(target).opacity,
            getComputedStyle(target).left].join(':');
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
        Some("0,0.3333333333333333,0.6666666666666666,1:true:0.5:45px")
    );
}

#[test]
fn keyframe_input_rejects_invalid_explicit_offsets_and_unsupported_composition() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        const target = document.createElement('div');
        const rejects = callback => { try { callback(); return false; }
            catch (error) { return error instanceof TypeError || error.name === 'NotSupportedError'; } };
        const results = [
            rejects(() => new KeyframeEffect(target, {opacity: [0, 1], offset: [0, 2]})),
            rejects(() => new KeyframeEffect(target, {opacity: [0, 1], offset: [0.8, 0.2]})),
            rejects(() => new KeyframeEffect(target, [{opacity: 0, composite: 'add'}])),
            rejects(() => new KeyframeEffect(target, {opacity: [0, 1], composite: 'accumulate'}))
        ];
        document.body.setAttribute('data-result', results.join(':'));
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

#[test]
fn effect_can_have_only_one_animation_owner_and_reassignment_removes_old_style() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity:.2'></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target, [{opacity: 0}, {opacity: 1}],
            {duration: 1000, fill: 'both'});
        const first = new Animation(effect);
        first.play(); first.pause(); first.currentTime = 500;
        const before = getComputedStyle(target).opacity;
        const second = new Animation(effect);
        const detached = first.effect === null && getComputedStyle(target).opacity === '0.2';
        second.play(); second.pause(); second.currentTime = 250;
        const after = getComputedStyle(target).opacity;
        second.cancel();
        document.body.setAttribute('data-result', [before, detached, after].join(':'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("0.5:true:0.25")
    );
}

#[test]
fn adding_a_property_to_running_effect_uses_its_underlying_value() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity:.2;left:10px'></div><script>
        const target = document.getElementById('target');
        const animation = target.animate([{opacity: 0}, {opacity: 1}],
            {duration: 1000, fill: 'both'});
        animation.pause(); animation.currentTime = 500;
        animation.effect.setKeyframes([
            {opacity: 0}, {opacity: 1, left: '50px'}
        ]);
        const value = getComputedStyle(target).left;
        const opacity = getComputedStyle(target).opacity;
        animation.cancel();
        document.body.setAttribute('data-result', [value, opacity].join(':'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("30px:0.5")
    );
}
