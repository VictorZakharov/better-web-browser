use super::*;

#[test]
fn fully_superseded_filling_animation_is_removed_and_dispatches_remove() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const older = target.animate([{opacity: 0}, {opacity: .5}],
            {duration: 100, fill: 'forwards'});
        older.finish();
        const newer = target.animate([{opacity: .5}, {opacity: 1}],
            {duration: 100, fill: 'forwards'});
        newer.finish();
        older.onremove = event => {
            const checks = [event instanceof AnimationPlaybackEvent,
                event.type === 'remove', event.currentTime === 100,
                older.replaceState === 'removed', newer.replaceState === 'active',
                !target.getAnimations().includes(older),
                target.getAnimations().includes(newer),
                getComputedStyle(target).opacity === '1'];
            document.body.setAttribute('data-result', checks.join(':'));
        };
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true:true:true:true:true:true:true")
    );
}

#[test]
fn persisted_or_partially_covered_effects_stay_in_the_animation_stack() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=first></div><div id=second></div><script>
        const first = document.getElementById('first');
        const second = document.getElementById('second');
        const persisted = first.animate([{opacity: 0}, {opacity: .5}],
            {duration: 100, fill: 'forwards'});
        persisted.finish(); persisted.persist();
        const replacement = first.animate([{opacity: .5}, {opacity: 1}],
            {duration: 100, fill: 'forwards'});
        replacement.finish();
        const partial = second.animate([
            {opacity: 0, left: '0px'}, {opacity: 1, left: '100px'}
        ], {duration: 100, fill: 'forwards'});
        partial.finish();
        const opacityOnly = second.animate([{opacity: 0}, {opacity: .2}],
            {duration: 100, fill: 'forwards'});
        opacityOnly.finish();
        requestAnimationFrame(() => {
            const checks = [persisted.replaceState === 'persisted',
                first.getAnimations().includes(persisted),
                partial.replaceState === 'active',
                second.getAnimations().includes(partial),
                getComputedStyle(second).left === '100px',
                getComputedStyle(second).opacity === '0.2'];
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
        Some("true:true:true:true:true:true")
    );
}

#[test]
fn commit_styles_requires_rendered_target_and_preserves_inline_style_after_cancel() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity:.1'></div>
        <div id=hidden style='display:none'><div id=child></div></div><script>
        const target = document.getElementById('target');
        const hidden = document.getElementById('child');
        const detached = document.createElement('div');
        const make = node => new Animation(new KeyframeEffect(node,
            [{opacity: 0}, {opacity: 1}], {duration: 100, fill: 'both'}));
        const animation = make(target);
        animation.play(); animation.pause(); animation.currentTime = 50;
        animation.commitStyles();
        const inline = target.style.opacity;
        animation.cancel();
        const retained = getComputedStyle(target).opacity;
        const rejects = node => {
            try { make(node).commitStyles(); return false; }
            catch (error) { return error.name === 'InvalidStateError'; }
        };
        document.body.setAttribute('data-result', [inline === '0.5',
            retained === '0.5', rejects(hidden), rejects(detached)].join(':'));
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
