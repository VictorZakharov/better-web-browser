use super::*;

fn fullscreen_runtime() -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(
        r#"<body><button id="target">Open</button><output id="status"></output><script>
            const target = document.getElementById('target');
            const status = document.getElementById('status');
            const order = [];
            document.addEventListener('fullscreenchange', event => {
                status.setAttribute('data-change', String(event.isTrusted));
                order.push('change');
            });
            document.addEventListener('fullscreenerror', event => {
                status.setAttribute('data-error', String(event.isTrusted));
                order.push('error');
            });
            target.addEventListener('click', () => target.requestFullscreen().then(() => {
                order.push('promise');
                status.textContent = [
                    document.fullscreenElement === target,
                    target.matches(':fullscreen'),
                    order.join(',')
                ].join('|');
            }, error => {
                order.push('rejection:' + error.name);
                status.textContent = order.join(',');
            }));
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#fullscreen".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[input]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}

#[test]
fn fullscreen_state_changes_only_after_browser_acknowledgement() {
    let (dom, mut runtime) = fullscreen_runtime();
    let target = dom.elements_named("button").next().unwrap();
    let request = runtime.dispatch_user_input(UserInputEvent::Pointer {
        target: Some(target.clone()),
        phase: "activate",
        button: 0,
        buttons: 0,
        x: 10.0,
        y: 10.0,
        activate: true,
        modifiers: UserInputModifiers::default(),
    });
    assert_eq!(request.outcome.fullscreen_actions.len(), 1);
    let action = request.outcome.fullscreen_actions[0];
    assert!(action.enter);
    assert!(!target.is_fullscreen());

    let response = runtime.dispatch_user_input(UserInputEvent::Fullscreen {
        request_id: action.request_id,
        disposition: "entered",
    });
    assert!(
        response.outcome.errors.is_empty(),
        "{:?}",
        response.outcome.errors
    );
    assert!(target.is_fullscreen());
    let status = dom.elements_named("output").next().unwrap();
    assert_eq!(status.text_content(), "true|true|change,promise");
    assert_eq!(status.attr("data-change").as_deref(), Some("true"));

    let exit = runtime.dispatch_user_input(UserInputEvent::Fullscreen {
        request_id: 0,
        disposition: "exited",
    });
    assert!(exit.outcome.errors.is_empty(), "{:?}", exit.outcome.errors);
    assert!(!target.is_fullscreen());
}

#[test]
fn denied_fullscreen_rejects_after_a_trusted_error_event() {
    let (dom, mut runtime) = fullscreen_runtime();
    let target = dom.elements_named("button").next().unwrap();
    let request = runtime.dispatch_user_input(UserInputEvent::Pointer {
        target: Some(target.clone()),
        phase: "activate",
        button: 0,
        buttons: 0,
        x: 10.0,
        y: 10.0,
        activate: true,
        modifiers: UserInputModifiers::default(),
    });
    let action = request.outcome.fullscreen_actions[0];

    let response = runtime.dispatch_user_input(UserInputEvent::Fullscreen {
        request_id: action.request_id,
        disposition: "denied",
    });
    assert!(
        response.outcome.errors.is_empty(),
        "{:?}",
        response.outcome.errors
    );
    assert!(!target.is_fullscreen());
    let status = dom.elements_named("output").next().unwrap();
    assert_eq!(status.text_content(), "error,rejection:NotAllowedError");
    assert_eq!(status.attr("data-error").as_deref(), Some("true"));
}

#[test]
fn disconnected_element_request_rejects_before_crossing_the_host_boundary() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            const detached = document.createElement('div');
            detached.requestFullscreen().catch(error => {
                document.querySelector('output').textContent = error.name;
            });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.fullscreen_actions.is_empty());
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "TypeError"
    );
}
