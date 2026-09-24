use super::*;

#[test]
fn animation_effect_and_timeline_expose_the_standard_prototype_hierarchy() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target, [{opacity: 0}, {opacity: 1}], 100);
        const timeline = new DocumentTimeline({originTime: -5000});
        const animation = new Animation(effect, timeline);
        const rejects = constructor => {
            try { new constructor(); return false; }
            catch (error) { return error instanceof TypeError; }
        };
        animation.id = 42;
        const checks = [effect instanceof AnimationEffect,
            Object.getPrototypeOf(KeyframeEffect.prototype) === AnimationEffect.prototype,
            Object.getPrototypeOf(DocumentTimeline.prototype) === AnimationTimeline.prototype,
            document.timeline instanceof AnimationTimeline,
            typeof AnimationEffect.prototype.getComputedTiming === 'function',
            timeline.currentTime > 4000,
            animation.id === '42',
            rejects(AnimationEffect), rejects(AnimationTimeline)];
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
        Some("true:true:true:true:true:true:true:true:true")
    );
}

#[test]
fn inactive_timeline_keeps_play_pending_until_a_document_timeline_is_attached() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target, [{opacity: 0}, {opacity: 1}],
            {duration: 1000, fill: 'both'});
        const animation = new Animation(effect, null);
        animation.play();
        const ready = animation.ready;
        const checks = [animation.timeline === null, animation.pending,
            animation.startTime === null, animation.currentTime === 0];
        queueMicrotask(() => {
            checks.push(animation.pending, animation.ready === ready);
            animation.timeline = document.timeline;
        });
        ready.then(value => {
            checks.push(value === animation, !animation.pending,
                animation.startTime !== null, animation.currentTime >= 0);
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
        Some("true:true:true:true:true:true:true:true:true:true")
    );
}

#[test]
fn animation_rejects_invalid_timeline_and_document_timeline_origin() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const effect = new KeyframeEffect(target, [{opacity: 0}, {opacity: 1}], 100);
        const animation = new Animation(effect);
        const rejects = callback => {
            try { callback(); return false; }
            catch (error) { return error instanceof TypeError; }
        };
        const checks = [
            rejects(() => new Animation(effect, {})),
            rejects(() => { animation.timeline = {}; }),
            rejects(() => new DocumentTimeline({originTime: Infinity})),
            rejects(() => new DocumentTimeline({originTime: NaN})),
            animation.timeline === document.timeline,
            animation.effect === effect
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
        Some("true:true:true:true:true:true")
    );
}
