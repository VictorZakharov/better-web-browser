use super::*;

#[test]
fn history_updates_capture_scroll_at_each_call_not_at_report_end() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><body><script>
            history.pushState(null, '', '#zero');
            scrollTo(0, 100.5);
            history.pushState(null, '', '#one');
            scrollTo(0, 300.25);
            history.replaceState(null, '', '#two');
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_viewport(800.0, 600.0);
    runtime.set_layout_content_height(2000.0);
    let script = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.viewport_scroll_y, Some(300.25));
    assert!(matches!(
        outcome.history_actions.as_slice(),
        [
            ScriptHistoryAction::Update { scroll_y: 0.0, .. },
            ScriptHistoryAction::Update {
                scroll_y: 100.5,
                ..
            },
            ScriptHistoryAction::Update {
                scroll_y: 300.25,
                ..
            }
        ]
    ));
}

#[test]
fn scroll_restoration_is_an_entry_mode_with_enum_validation() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const result = [history.scrollRestoration];
            history.scrollRestoration = 'manual';
            history.pushState({ step: 1 }, '', '#one');
            history.replaceState({ step: 2 }, '', '#two');
            result.push(history.scrollRestoration);
            for (const invalid of ['automatic', Symbol('invalid')]) {
                try { history.scrollRestoration = invalid; result.push('accepted'); }
                catch (error) { result.push(error.name); }
            }
            result.push(history.scrollRestoration);
            document.querySelector('output').textContent = result.join('|');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "auto|manual|TypeError|TypeError|manual"
    );
    assert!(matches!(
        outcome.history_actions.as_slice(),
        [
            ScriptHistoryAction::SetScrollRestoration {
                mode: crate::renderer_protocol::ScrollRestorationMode::Manual,
            },
            ScriptHistoryAction::Update { replace: false, .. },
            ScriptHistoryAction::Update { replace: true, .. }
        ]
    ));
}

#[test]
fn traversal_exposes_target_restoration_mode_before_popstate() {
    let dom = dom::parse_with_scripting(
        r#"<body><output></output><script>
            addEventListener('popstate', () => {
                document.querySelector('output').textContent = history.scrollRestoration;
            });
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(runtime.execute_initial(&[input]).errors.is_empty());
    let traversal = runtime.apply_history_traversal(
        "https://example.com/#step",
        None,
        2,
        1,
        crate::renderer_protocol::ScrollRestorationMode::Manual,
    );
    assert!(traversal.errors.is_empty(), "{:?}", traversal.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "manual"
    );
}

#[test]
fn history_state_is_a_storage_clone_and_failed_updates_leave_the_entry_intact() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const source = { step: 1 };
            history.pushState(source, '', '/first');
            source.step = 2;
            const stored = history.state;
            let cloneError = '';
            try { history.pushState({ callback() {} }, '', '/rejected'); }
            catch (error) { cloneError = error.name; }
            document.querySelector('output').textContent = [
                stored.step, stored === history.state, stored !== source,
                cloneError, location.href, history.length, history.state === stored
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.history_actions.len(), 1);
    let ScriptHistoryAction::Update {
        state: Some(serialized),
        ..
    } = &outcome.history_actions[0]
    else {
        panic!("expected a serialized History update");
    };
    let snapshot: serde_json::Value = serde_json::from_str(serialized).unwrap();
    assert_eq!(snapshot["v"][0][1], 1);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "1|true|true|DataCloneError|https://example.com/first|2|true"
    );
}

#[test]
fn history_traversal_restores_state_before_popstate_and_queues_hashchange() {
    let dom = dom::parse_with_scripting(
        r#"<body><output></output><script>
            const events = [];
            addEventListener('popstate', event => {
                events.push(['popstate', event instanceof PopStateEvent, event.isTrusted,
                    event.state === history.state, history.state?.step ?? null,
                    document.URL === location.href, location.href].join(':'));
                document.querySelector('output').textContent = events.join(';');
            });
            addEventListener('hashchange', event => {
                events.push(['hashchange', event instanceof HashChangeEvent, event.isTrusted,
                    event.oldURL, event.newURL].join(':'));
                document.querySelector('output').textContent = events.join(';');
            });
            history.pushState({ step: 1 }, '', '#first');
            history.state.step = 9;
            document.querySelector('output').textContent = [
                events.length, typeof __applyHistoryTraversal, typeof __setHistoryMetrics
            ].join('|');
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[input]);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.history_actions.len(), 1);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "0|undefined|undefined"
    );
    let traversal = runtime.apply_history_traversal(
        "https://example.com/",
        None,
        2,
        0,
        crate::renderer_protocol::ScrollRestorationMode::Auto,
    );
    assert!(traversal.errors.is_empty(), "{:?}", traversal.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "popstate:true:true:true::true:https://example.com/"
    );
    assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "popstate:true:true:true::true:https://example.com/;hashchange:true:true:https://example.com/#first:https://example.com/"
    );
    let serialized = r#"{"t":"object","id":1,"n":false,"v":[["step",1]]}"#;
    let forward = runtime.apply_history_traversal(
        "https://example.com/#first",
        Some(serialized),
        2,
        1,
        crate::renderer_protocol::ScrollRestorationMode::Auto,
    );
    assert!(forward.errors.is_empty(), "{:?}", forward.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "popstate:true:true:true::true:https://example.com/;hashchange:true:true:https://example.com/#first:https://example.com/;popstate:true:true:true:1:true:https://example.com/#first"
    );
    assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "popstate:true:true:true::true:https://example.com/;hashchange:true:true:https://example.com/#first:https://example.com/;popstate:true:true:true:1:true:https://example.com/#first;hashchange:true:true:https://example.com/:https://example.com/#first"
    );
}

#[test]
fn fragment_navigation_clears_classic_state_and_fires_popstate_before_hashchange() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            history.replaceState({ step: 1 }, '');
            const events = [];
            addEventListener('popstate', event => events.push([
                event.isTrusted, event.state === null, history.state === null, location.hash
            ].join(':')));
            addEventListener('hashchange', () => events.push('hashchange'));
            location.hash = 'section';
            document.querySelector('output').textContent = [
                events.join(','), history.length
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.history_actions.len(), 2);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true:true:true:#section|2"
    );
}

#[test]
fn traversal_requests_are_ordered_and_do_not_change_state_synchronously() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            let popstates = 0;
            addEventListener('popstate', () => popstates++);
            history.pushState({ step: 1 }, '', '/one');
            history.back();
            history.forward();
            history.go(-99);
            document.querySelector('output').textContent = [
                location.href, history.length, history.state.step, popstates
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.history_actions.len(), 4);
    assert!(matches!(
        outcome.history_actions.as_slice(),
        [
            ScriptHistoryAction::Update { .. },
            ScriptHistoryAction::Traverse { delta: -1 },
            ScriptHistoryAction::Traverse { delta: 1 },
            ScriptHistoryAction::Traverse { delta: -99 },
        ]
    ));
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "https://example.com/one|2|1|0"
    );
}

#[test]
fn history_argument_conversion_and_required_title_do_not_queue_invalid_updates() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const failures = [];
            try { history.pushState({ step: 1 }); }
            catch (error) { failures.push(error.name); }
            try { history.go(1n); }
            catch (error) { failures.push(error.name); }
            try { history.go(Symbol()); }
            catch (error) { failures.push(error.name); }
            try { history.pushState({ step: 1 }, Symbol()); }
            catch (error) { failures.push(error.name); }
            try { history.pushState({ step: 1 }, '', Symbol()); }
            catch (error) { failures.push(error.name); }
            history.go();
            history.go(4294967297);
            document.querySelector('output').textContent = failures.join(',');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "TypeError,TypeError,TypeError,TypeError,TypeError"
    );
    assert!(matches!(
        outcome.history_actions.as_slice(),
        [
            ScriptHistoryAction::Traverse { delta: 0 },
            ScriptHistoryAction::Traverse { delta: 1 },
        ]
    ));
}

#[test]
fn history_rejects_malformed_urls_and_credential_rewrites() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const failures = [];
            for (const url of ['http://[bad', 'https://user:pass@example.com/']) {
                try { history.pushState({ step: 1 }, '', url); }
                catch (error) { failures.push(error.name); }
            }
            document.querySelector('output').textContent = [
                failures.join(','), location.href, history.length, history.state
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.history_actions.is_empty());
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "SecurityError,SecurityError|https://example.com/|1|"
    );
}

#[test]
fn history_url_security_error_is_a_dom_exception() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            try { history.pushState(null, '', 'https://other.example/'); }
            catch (error) {
                document.querySelector('output').textContent = [
                    error instanceof DOMException, error.name, error.code
                ].join('|');
            }
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.history_actions.is_empty());
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|SecurityError|18"
    );
}

#[test]
fn empty_history_url_preserves_the_current_fragment() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            location.hash = 'part';
            history.pushState({ step: 1 }, '', '');
            document.querySelector('output').textContent = [
                location.href, history.length, history.state.step
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.history_actions.len(), 2);
    assert!(matches!(
        outcome.history_actions.as_slice(),
        [
            ScriptHistoryAction::Update { url: fragment, state: None, .. },
            ScriptHistoryAction::Update { url: pushed, state: Some(_), .. },
        ] if fragment == "https://example.com/#part" && pushed == fragment
    ));
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "https://example.com/#part|3|1"
    );
}

#[test]
fn a_new_document_uses_tab_history_metrics_and_discarded_forward_entries() {
    let dom = dom::parse_with_scripting(
        r#"<body><output></output><script>
            const before = [history.length, history.state.step].join(':');
            history.pushState({ step: 4 }, '', '/new');
            const after = [history.length, history.state.step].join(':');
            document.querySelector('output').textContent = [before, after].join('|');
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = |code: String| ScriptInput {
        source_url: "https://example.com/".into(),
        code,
        node: script.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(runtime.execute_initial(&[]).errors.is_empty());
    let initial_state = r#"{"t":"object","id":1,"n":false,"v":[["step",3]]}"#;
    runtime
        .set_history_metrics(
            5,
            2,
            Some(initial_state),
            crate::renderer_protocol::ScrollRestorationMode::Auto,
        )
        .unwrap();
    let first = runtime.execute_additional_with_loader(&[input(script.text_content())], None);

    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert_eq!(first.history_actions.len(), 1);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "5:3|4:4"
    );
    runtime
        .set_history_metrics(
            512,
            511,
            None,
            crate::renderer_protocol::ScrollRestorationMode::Auto,
        )
        .unwrap();
    let second = runtime.execute_additional_with_loader(
        &[input("history.pushState(null, '', '/at-capacity'); document.querySelector('output').textContent += '|' + history.length;".into())],
        None,
    );
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    assert_eq!(second.history_actions.len(), 1);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "5:3|4:4|512"
    );
}
