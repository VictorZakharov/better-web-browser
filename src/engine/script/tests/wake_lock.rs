use super::*;

#[test]
fn browser_acknowledgement_creates_branded_sentinel_and_release_is_trusted() {
    let html = r#"<body><output></output><script>
        const output = document.querySelector('output');
        const manager = navigator.wakeLock;
        output.dataset.manager = String(manager === navigator.wakeLock);
        manager.request().then(sentinel => {
            window.sentinel = sentinel;
            output.textContent = [sentinel.type, sentinel.released,
                sentinel instanceof WakeLockSentinel].join('|');
            sentinel.onrelease = event => {
                output.textContent = ['released', sentinel.released, event.isTrusted].join('|');
            };
        }, error => { output.textContent = error.name; });
    </script></body>"#;
    let dom = dom::parse_with_scripting(html, true);
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let initial = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.test/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.wake_lock_actions.len(), 1);
    let action = initial.wake_lock_actions[0];
    assert_eq!(
        action.action,
        crate::renderer_protocol::WakeLockAction::Acquire
    );
    let output = dom.elements_named("output").next().unwrap();
    assert_eq!(output.attr("data-manager").as_deref(), Some("true"));
    assert_eq!(output.text_content(), "");

    let granted = runtime.dispatch_user_input(UserInputEvent::WakeLock {
        request_id: action.request_id,
        disposition: "granted",
    });
    assert!(
        granted.outcome.errors.is_empty(),
        "{:?}",
        granted.outcome.errors
    );
    assert_eq!(output.text_content(), "screen|false|true");
    let released = runtime.dispatch_user_input(UserInputEvent::WakeLock {
        request_id: action.request_id,
        disposition: "released",
    });
    assert!(
        released.outcome.errors.is_empty(),
        "{:?}",
        released.outcome.errors
    );
    assert_eq!(output.text_content(), "released|true|true");
}

#[test]
fn insecure_document_does_not_expose_wake_lock() {
    let dom = dom::parse_with_scripting(
        "<body><script>document.body.dataset.hasWakeLock = String('wakeLock' in navigator)</script></body>",
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "http://example.test/");
    let result = runtime.execute_initial(&[ScriptInput {
        source_url: "http://example.test/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-has-wake-lock")
            .as_deref(),
        Some("false")
    );
}

#[test]
fn hiding_the_document_releases_an_acknowledged_lock() {
    let html = r#"<body><output></output><script>
        navigator.wakeLock.request().then(lock => {
            lock.onrelease = event => {
                document.querySelector('output').textContent =
                    [lock.released, event.isTrusted, document.hidden].join('|');
            };
        });
    </script></body>"#;
    let dom = dom::parse_with_scripting(html, true);
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let initial = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.test/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let id = initial.wake_lock_actions[0].request_id;
    let granted = runtime.dispatch_user_input(UserInputEvent::WakeLock {
        request_id: id,
        disposition: "granted",
    });
    assert!(
        granted.outcome.errors.is_empty(),
        "{:?}",
        granted.outcome.errors
    );
    let hidden = runtime.dispatch_user_input(UserInputEvent::Lifecycle {
        state: "hidden",
        previous: "active",
    });
    assert!(
        hidden.outcome.errors.is_empty(),
        "{:?}",
        hidden.outcome.errors
    );
    assert_eq!(hidden.outcome.wake_lock_actions.len(), 1);
    assert_eq!(
        hidden.outcome.wake_lock_actions[0].action,
        crate::renderer_protocol::WakeLockAction::Release
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|true"
    );
}
