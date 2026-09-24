use super::*;

#[test]
fn linear_easing_distributes_missing_positions_and_interpolates_segments() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target, [{opacity: 0}, {opacity: 1}],
            {duration: 100, fill: 'both', easing: 'linear(0, .2, .4 60%, 1)'});
        const animation = new Animation(effect);
        animation.play(); animation.pause();
        const values = [];
        for (const time of [0, 30, 45, 60, 80, 100]) {
            animation.currentTime = time;
            values.push(Number(effect.getComputedTiming().progress.toFixed(4)));
        }
        animation.cancel();
        document.body.setAttribute('data-result', values.join(','));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("0,0.2,0.3,0.4,0.7,1")
    );
}

#[test]
fn linear_easing_handles_plateaus_discontinuities_and_out_of_order_inputs() {
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
            sample('linear(0, .5 25% 75%, 1)', 50) === .5,
            sample('linear(0 0%, .3 50%, .7 50%, 1 100%)', 50) === .7,
            sample('linear(0 0%, .3 50%, .7 25%, 1 100%)', 50) === .7,
            sample('linear(.4)', 50) === .4,
            sample('linear(0 0%, 1 100%)', 25) === .25
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
        Some("true:true:true:true:true")
    );
}

#[test]
fn linear_easing_is_available_per_keyframe_and_invalid_syntax_preserves_effect() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target,
            [{opacity: 0, easing: 'linear(0, 0 50%, 1)'}, {opacity: 1}],
            {duration: 100, fill: 'both'});
        const animation = new Animation(effect);
        animation.play(); animation.pause(); animation.currentTime = 25;
        const first = getComputedStyle(target).opacity;
        animation.currentTime = 75;
        const second = getComputedStyle(target).opacity;
        const old = JSON.stringify(effect.getKeyframes());
        const rejects = easing => {
            try { effect.setKeyframes([{opacity: 0, easing}, {opacity: 1}]); return false; }
            catch (error) { return error instanceof TypeError; }
        };
        const checks = [first === '0', second === '0.5',
            rejects('linear()'), rejects('linear(0 2px, 1)'),
            rejects('linear(0 20% 40% 60%, 1)'),
            JSON.stringify(effect.getKeyframes()) === old];
        animation.cancel();
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
        Some("true:true:true:true:true:true")
    );
}

#[test]
fn linear_easing_output_can_extrapolate_numeric_and_transform_keyframes() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const animation = target.animate([
            {left: '0px', transform: 'translateX(0px)'},
            {left: '100px', transform: 'translateX(100px)'}
        ], {duration: 100, fill: 'both', easing: 'linear(-.5, 1.5)'});
        animation.pause();
        animation.currentTime = 0;
        const before = [getComputedStyle(target).left,
            getComputedStyle(target).transform];
        animation.currentTime = 100;
        const after = [getComputedStyle(target).left,
            getComputedStyle(target).transform];
        animation.cancel();
        document.body.setAttribute('data-result',
            [before[0] === '-50px', before[1] === 'translate(-50px, 0px)',
             after[0] === '150px', after[1] === 'translate(150px, 0px)'].join(':'));
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
fn cubic_bezier_overshoot_uses_endpoint_tangents_beyond_the_domain() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const animation = target.animate([
            {left: '0px', easing: 'cubic-bezier(.5, -1, .5, 2)'},
            {left: '100px'}
        ], {duration: 100, fill: 'both', easing: 'linear(-.5, 1.5)'});
        animation.pause();
        animation.currentTime = 0;
        const before = getComputedStyle(target).left;
        animation.currentTime = 100;
        const after = getComputedStyle(target).left;
        animation.cancel();
        document.body.setAttribute('data-result', [before === '100px',
            after === '0px'].join(':'));
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
