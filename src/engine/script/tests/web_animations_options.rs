use super::*;

#[test]
fn element_animate_honors_id_and_timeline_options_with_web_idl_conversion() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const timeline = new DocumentTimeline({originTime: -2000});
        const animation = target.animate([{opacity: 0}, {opacity: 1}],
            {duration: 100, fill: 'both', id: 42, timeline});
        const defaultAnimation = target.animate([{opacity: 0}, {opacity: 1}], 100);
        const nullId = target.animate([{opacity: 0}, {opacity: 1}],
            {duration: 100, id: null});
        const checks = [animation.id === '42', animation.timeline === timeline,
            defaultAnimation.id === '', defaultAnimation.timeline === document.timeline,
            nullId.id === 'null', animation.pending];
        animation.cancel(); defaultAnimation.cancel(); nullId.cancel();
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
fn subtree_queries_include_descendants_but_not_siblings_or_removed_effects() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=parent><div id=child></div></div>
        <div id=sibling></div><script>
        const parent = document.getElementById('parent');
        const child = document.getElementById('child');
        const sibling = document.getElementById('sibling');
        const animate = target => target.animate([{opacity: 0}, {opacity: 1}],
            {duration: 100, fill: 'both'});
        const own = animate(parent), nested = animate(child), outside = animate(sibling);
        const direct = parent.getAnimations();
        const subtree = parent.getAnimations({subtree: true});
        const checks = [direct.length === 1 && direct[0] === own,
            subtree.length === 2 && subtree.includes(own) && subtree.includes(nested),
            !subtree.includes(outside),
            child.getAnimations({subtree: true}).length === 1,
            document.getAnimations().length === 3];
        nested.cancel();
        checks.push(parent.getAnimations({subtree: true}).length === 1);
        own.cancel(); outside.cancel();
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
fn subtree_query_follows_shadow_including_ancestry() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=host></div><script>
        const host = document.getElementById('host');
        const root = host.attachShadow({mode: 'open'});
        const child = document.createElement('span');
        root.appendChild(child);
        const animation = child.animate([{opacity: 0}, {opacity: 1}],
            {duration: 100, fill: 'both'});
        const checks = [host.getAnimations().length === 0,
            host.getAnimations({subtree: true}).includes(animation),
            child.getAnimations().includes(animation)];
        animation.cancel();
        checks.push(host.getAnimations({subtree: true}).length === 0);
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
        Some("true:true:true:true")
    );
}

#[test]
fn element_animate_with_null_timeline_waits_for_timeline_attachment() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target></div><script>
        const target = document.getElementById('target');
        const animation = target.animate([{opacity: 0}, {opacity: 1}],
            {duration: 1000, fill: 'both', timeline: null});
        const waiting = animation.ready;
        const checks = [animation.timeline === null, animation.pending,
            animation.startTime === null, animation.currentTime === 0,
            waiting === animation.ready, target.getAnimations().includes(animation)];
        Promise.resolve().then(() => {
            checks.push(animation.pending, animation.ready === waiting);
            animation.timeline = document.timeline;
            checks.push(animation.timeline === document.timeline,
                animation.ready === waiting);
            waiting.then(value => {
                checks.push(value === animation, !animation.pending,
                    animation.startTime !== null, animation.playState === 'running');
                animation.cancel();
                document.body.setAttribute('data-result', checks.join(':'));
            });
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
        Some("true:true:true:true:true:true:true:true:true:true:true:true:true:true")
    );
}
