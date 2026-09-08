use super::*;

fn start_observer_script(source: &str) -> (dom::Dom, ScriptRuntime, NodeRef) {
    let dom = dom::parse_with_scripting(
        &format!(
            "<body><div id='target'></div><div id='status'>waiting</div><script>{source}</script></body>"
        ),
        true,
    );
    let target = dom.elements_named("div").next().unwrap();
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#observer-lifecycle".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime, target)
}

fn publish_target(runtime: &mut ScriptRuntime, target: &NodeRef, top: f32) -> ScriptOutcome {
    runtime.set_layout_geometry(&HashMap::from([(
        target.id(),
        RectF {
            x: 10.0,
            y: top,
            width: 100.0,
            height: 50.0,
        },
    )]));
    let outcome = runtime.notify_layout_changed();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    outcome
}

fn status(dom: &dom::Dom) -> String {
    dom.elements_named("div").nth(1).unwrap().text_content()
}

#[test]
fn disconnected_observer_can_observe_again_before_the_first_layout() {
    let (dom, mut runtime, target) = start_observer_script(
        r#"
        const target = document.getElementById('target');
        const results = [];
        const observer = new IntersectionObserver(entries => {
            results.push(...entries.map(entry => entry.isIntersecting));
            document.getElementById('status').textContent = results.join(',');
        });
        observer.observe(target);
        observer.disconnect();
        observer.observe(target);
        observer.observe(target);
        "#,
    );

    assert_eq!(status(&dom), "waiting");
    publish_target(&mut runtime, &target, 10.0);
    assert_eq!(status(&dom), "true");
    publish_target(&mut runtime, &target, 1000.0);
    assert_eq!(status(&dom), "true,false");
    publish_target(&mut runtime, &target, 10.0);
    assert_eq!(status(&dom), "true,false,true");
}

#[test]
fn reobserving_inside_a_callback_waits_for_the_next_rendering_update() {
    for stop in ["observer.disconnect()", "observer.unobserve(target)"] {
        let (dom, mut runtime, target) = start_observer_script(&format!(
            r#"
            const target = document.getElementById('target');
            const results = [];
            const observer = new IntersectionObserver(entries => {{
                results.push(...entries.map(entry => entry.isIntersecting));
                document.getElementById('status').textContent = results.join(',');
                if (results.length === 1) {{
                    {stop};
                    observer.observe(target);
                }}
            }});
            observer.observe(target);
            "#,
        ));

        publish_target(&mut runtime, &target, 10.0);
        assert_eq!(status(&dom), "true", "revisited observer after {stop}");
        publish_target(&mut runtime, &target, 10.0);
        assert_eq!(
            status(&dom),
            "true,true",
            "missed initial entry after {stop}"
        );
        publish_target(&mut runtime, &target, 1000.0);
        assert_eq!(
            status(&dom),
            "true,true,false",
            "lost observer after {stop}"
        );
    }
}

#[test]
fn removing_one_observer_does_not_interrupt_other_active_observers() {
    let (dom, mut runtime, target) = start_observer_script(
        r#"
        const target = document.getElementById('target');
        const results = [];
        const observer = new IntersectionObserver(() => {
            observer.disconnect();
        });
        observer.observe(target);
        new IntersectionObserver(entries => {
            results.push(...entries.map(entry => entry.isIntersecting));
            document.getElementById('status').textContent = results.join(',');
        }).observe(target);
        "#,
    );

    publish_target(&mut runtime, &target, 10.0);
    assert_eq!(status(&dom), "true");
    publish_target(&mut runtime, &target, 1000.0);
    assert_eq!(status(&dom), "true,false");
}

#[test]
fn throwing_observer_reports_once_and_preserves_other_observers_and_reconnection() {
    for cancel_error in [false, true] {
        let source = r#"
            const target = document.getElementById('target');
            const fault = new Error('observer failure');
            const errors = [], results = [];
            let firstCalls = 0;
            const publishStatus = () => {
                document.getElementById('status').textContent =
                    errors.join(',') + ';' + firstCalls + ';' + results.join(',');
            };
            addEventListener('error', event => {
                errors.push(event.message + ':' + event.isTrusted + ':' + (event.error === fault));
                if (CANCEL_ERROR) event.preventDefault();
            });
            const first = new IntersectionObserver(() => {
                firstCalls++;
                if (firstCalls === 1) {
                    first.disconnect();
                    first.observe(target);
                    throw fault;
                }
                publishStatus();
            });
            first.observe(target);
            new IntersectionObserver(entries => {
                results.push(...entries.map(entry => entry.isIntersecting));
                publishStatus();
            }).observe(target);
        "#
        .replace("CANCEL_ERROR", if cancel_error { "true" } else { "false" });
        let (dom, mut runtime, target) = start_observer_script(&source);

        let initial = publish_target(&mut runtime, &target, 10.0);
        assert_eq!(status(&dom), "observer failure:true:true;1;true");
        assert_eq!(
            initial.console.len(),
            usize::from(!cancel_error),
            "{:?}",
            initial.console
        );
        if !cancel_error {
            assert!(
                initial.console[0]
                    .contains("Uncaught IntersectionObserver exception: observer failure")
            );
        }
        // Re-observation is deferred to the next checkpoint, not lost when its callback throws.
        let reconnected = publish_target(&mut runtime, &target, 10.0);
        assert!(reconnected.console.is_empty());
        assert_eq!(status(&dom), "observer failure:true:true;2;true");
        let moved = publish_target(&mut runtime, &target, 1000.0);
        assert!(moved.console.is_empty());
        assert_eq!(status(&dom), "observer failure:true:true;3;true,false");
    }
}
