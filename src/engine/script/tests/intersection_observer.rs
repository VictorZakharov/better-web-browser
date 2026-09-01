use super::*;

#[test]
fn intersection_observer_is_exposed_on_the_window_global() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status"></div><script>
            document.getElementById('status').textContent = String(
                window === globalThis &&
                'IntersectionObserver' in window &&
                typeof window.IntersectionObserver === 'function' &&
                'IntersectionObserverEntry' in window &&
                'intersectionRatio' in window.IntersectionObserverEntry.prototype &&
                'isIntersecting' in window.IntersectionObserverEntry.prototype
            );
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn intersection_observer_delivers_an_asynchronous_initial_entry() {
    let dom = dom::parse_with_scripting(
        r#"<body><div id="target"></div><div id="status">waiting</div><script>
            let synchronous = true;
            const target = document.getElementById('target');
            const observer = new IntersectionObserver((entries, current) => {
                const entry = entries[0];
                const valid = !synchronous && current === observer && entry.target === target &&
                    entry.isIntersecting && entry.intersectionRatio === 1 &&
                    entry.rootBounds.width === 1792 && entry.rootBounds.height === 740 &&
                    observer.root === null && observer.rootMargin === '10px 20% 10px 20%' &&
                    observer.thresholds.join(',') === '0,0.5';
                document.getElementById('status').textContent = valid ? 'yes' : 'invalid';
            }, { rootMargin: '10px 20%', threshold: [0.5, 0] });
            observer.observe(target);
            synchronous = false;
        </script></body>"#,
        true,
    );
    let target = dom.elements_named("div").next().unwrap();
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#inline".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").nth(1).unwrap().text_content(),
        "waiting"
    );

    runtime.set_layout_geometry(&HashMap::from([(
        target.id(),
        RectF {
            x: 40.0,
            y: 30.0,
            width: 320.0,
            height: 180.0,
        },
    )]));
    let outcome = runtime.notify_layout_changed();

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").nth(1).unwrap().text_content(),
        "yes"
    );
}

#[test]
fn intersection_observer_rendering_task_is_not_starved_by_timer_backlog() {
    let dom = dom::parse_with_scripting(
        r#"<body><div id="target"></div><div id="status">waiting</div><script>
            const target = document.getElementById('target');
            for (let index = 0; index < 128; index++) {
                setTimeout(() => target.setAttribute('data-timer', String(index)), 0);
            }
            new IntersectionObserver(entries => {
                if (entries[0].isIntersecting)
                    document.getElementById('status').textContent = 'delivered';
            }).observe(target);
        </script></body>"#,
        true,
    );
    let target = dom.elements_named("div").next().unwrap();
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let queued = runtime.execute_additional_with_loader(
        &[ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: script.text_content(),
            node: script,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: false,
        }],
        None,
    );

    assert!(queued.errors.is_empty(), "{:?}", queued.errors);
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    runtime.set_layout_geometry(&HashMap::from([(
        target.id(),
        RectF {
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 50.0,
        },
    )]));

    let observed = runtime.notify_layout_changed();

    assert!(observed.errors.is_empty(), "{:?}", observed.errors);
    assert_eq!(
        dom.elements_named("div").nth(1).unwrap().text_content(),
        "delivered"
    );
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
}

#[test]
fn intersection_observer_validates_inputs_and_honors_disconnect() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="target"></div><div id="status">waiting</div><script>
            let invalidCallback = false;
            let invalidThreshold = false;
            try { new IntersectionObserver(null); } catch (error) { invalidCallback = error instanceof TypeError; }
            try { new IntersectionObserver(() => {}, { threshold: 2 }); }
            catch (error) { invalidThreshold = error instanceof RangeError; }
            const observer = new IntersectionObserver(() => {
                document.getElementById('status').textContent = 'callback ran';
            });
            observer.observe(document.getElementById('target'));
            observer.disconnect();
            setTimeout(() => {
                document.getElementById('status').textContent = invalidCallback && invalidThreshold ? 'yes' : 'invalid';
            }, 1);
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").nth(1).unwrap().text_content(),
        "yes"
    );
}
