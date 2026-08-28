use super::*;

#[test]
fn external_script_tasks_allow_a_rendering_callback_before_the_next_script() {
    let dom = dom::parse_with_scripting(
        r#"<body><script src="first.js"></script><script src="second.js"></script></body>"#,
        true,
    );
    let nodes = dom.elements_named("script").collect::<Vec<_>>();
    let scripts = [
        input(
            &nodes[0],
            "first.js",
            "requestAnimationFrame(() => document.body.setAttribute('data-frame', 'ran'));",
            false,
        ),
        input(
            &nodes[1],
            "second.js",
            "document.body.setAttribute('data-observed', document.body.getAttribute('data-frame') || 'missing');",
            true,
        ),
    ];
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");

    let outcome = runtime.execute_initial(&scripts);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-observed"))
            .as_deref(),
        Some("ran")
    );
}

#[test]
fn elapsed_time_makes_a_timer_ready_without_running_its_task() {
    let dom = dom::parse_with_scripting(
        r#"<body><div>waiting</div><script>
            setTimeout(() => document.querySelector('div').textContent = 'done', 2000);
        </script></body>"#,
        true,
    );
    let scripts = script_inputs(&dom);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(runtime.execute_initial(&scripts).errors.is_empty());

    runtime.elapse_time(Duration::from_millis(500));

    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "waiting"
    );
    assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "done"
    );
}

#[test]
fn retained_runtime_yields_between_dynamic_script_tasks() {
    let dom = dom::parse_with_scripting(
        "<html><head></head><body><div>waiting</div><script></script></body></html>",
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let queues_two = input(
        &node,
        "async.js",
        r#"for (const src of ['/first.js', '/second.js']) {
            const script = document.createElement('script');
            script.src = src;
            document.head.appendChild(script);
        }"#,
        false,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(runtime.execute_initial(&[]).errors.is_empty());
    let queued = runtime.execute_additional_with_loader(&[queues_two], None);
    assert!(queued.errors.is_empty(), "{:?}", queued.errors);
    assert!(runtime.has_pending_dynamic_scripts());
    let mut loader = |url: &str, _, _| {
        let value = if url.ends_with("first.js") {
            "first"
        } else {
            "second"
        };
        Ok(format!(
            "document.querySelector('div').textContent = '{value}';"
        ))
    };
    let first = runtime.advance_time_with_loader(Duration::ZERO, 8, Some(&mut loader));
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert!(runtime.has_pending_dynamic_scripts());
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "first"
    );
    let second = runtime.advance_time_with_loader(Duration::ZERO, 8, Some(&mut loader));
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    assert!(!runtime.has_pending_dynamic_scripts());
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "second"
    );
}

#[test]
fn retained_runtime_interleaves_ready_timers_with_dynamic_scripts() {
    let dom = dom::parse_with_scripting(
        "<html><head></head><body><script></script></body></html>",
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let queues_work = input(
        &node,
        "async.js",
        r#"window.order = [];
            setTimeout(() => {
                order.push('timer-one');
                setTimeout(() => order.push('timer-two'), 0);
            }, 0);
            for (const src of ['/first.js', '/second.js']) {
                const script = document.createElement('script');
                script.src = src;
                document.head.appendChild(script);
            }"#,
        false,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(runtime.execute_initial(&[]).errors.is_empty());
    let queued = runtime.execute_additional_with_loader(&[queues_work], None);
    assert!(queued.errors.is_empty(), "{:?}", queued.errors);
    let mut loader = |url: &str, _, _| {
        let value = if url.ends_with("first.js") {
            "dynamic-one"
        } else {
            "dynamic-two"
        };
        Ok(format!("order.push('{value}');"))
    };

    for _ in 0..4 {
        let outcome = runtime.advance_time_with_loader(Duration::ZERO, 8, Some(&mut loader));
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
    let outcome = runtime.execute_additional_with_loader(
        &[input(
            &node,
            "result.js",
            "document.body.setAttribute('data-order', order.join(','));",
            false,
        )],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-order"))
            .as_deref(),
        Some("timer-one,dynamic-one,timer-two,dynamic-two")
    );
}
