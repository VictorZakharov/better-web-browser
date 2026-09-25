use super::*;

fn lock_runtime() -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(
        r#"<body><canvas id="target"></canvas><output></output><script>
            const target = document.getElementById('target');
            const status = document.querySelector('output');
            const order = [];
            document.onpointerlockchange = event => order.push('change:' + event.isTrusted);
            document.onpointerlockerror = event => order.push('error:' + event.isTrusted);
            target.onclick = () => target.requestPointerLock().then(() => {
                order.push('resolve');
                status.textContent = [document.pointerLockElement === target,
                    order.join(',')].join('|');
            }, error => {
                order.push('reject:' + error.name);
                status.textContent = order.join(',');
            });
            target.onmousemove = event => status.setAttribute('data-motion',
                [event.movementX, event.movementY, event.clientX, event.clientY].join(','));
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#pointer-lock".into(),
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

fn click(runtime: &mut ScriptRuntime, target: &dom::NodeRef) -> UserInputResult {
    runtime.dispatch_user_input(UserInputEvent::Pointer {
        target: Some(target.clone()),
        phase: "activate",
        button: 0,
        buttons: 0,
        x: 30.0,
        y: 45.0,
        activate: true,
        modifiers: UserInputModifiers::default(),
    })
}

#[test]
fn pointer_lock_changes_only_after_browser_acknowledgement() {
    let (dom, mut runtime) = lock_runtime();
    let target = dom.elements_named("canvas").next().unwrap();
    let status = dom.elements_named("output").next().unwrap();
    let request = click(&mut runtime, &target);
    assert_eq!(request.outcome.pointer_lock_actions.len(), 1);
    let action = request.outcome.pointer_lock_actions[0];
    assert_eq!(action.target, Some(target.id()));
    assert_eq!(status.text_content(), "");

    let entered = runtime.dispatch_user_input(UserInputEvent::PointerLock {
        request_id: action.request_id,
        disposition: "entered",
    });
    assert!(
        entered.outcome.errors.is_empty(),
        "{:?}",
        entered.outcome.errors
    );
    assert_eq!(status.text_content(), "true|change:true,resolve");

    let motion = runtime.dispatch_user_input(UserInputEvent::Pointer {
        target: Some(target.clone()),
        phase: "lockedmove",
        button: 0,
        buttons: 0,
        x: 7.0,
        y: -3.0,
        activate: false,
        modifiers: UserInputModifiers::default(),
    });
    assert!(
        motion.outcome.errors.is_empty(),
        "{:?}",
        motion.outcome.errors
    );
    assert_eq!(status.attr("data-motion").as_deref(), Some("7,-3,30,45"));

    let exited = runtime.dispatch_user_input(UserInputEvent::PointerLock {
        request_id: 0,
        disposition: "exited",
    });
    assert!(
        exited.outcome.errors.is_empty(),
        "{:?}",
        exited.outcome.errors
    );
}

#[test]
fn denied_lock_rejects_after_trusted_error_and_leaves_pointer_unlocked() {
    let (dom, mut runtime) = lock_runtime();
    let target = dom.elements_named("canvas").next().unwrap();
    let status = dom.elements_named("output").next().unwrap();
    let request = click(&mut runtime, &target);
    let action = request.outcome.pointer_lock_actions[0];
    let denied = runtime.dispatch_user_input(UserInputEvent::PointerLock {
        request_id: action.request_id,
        disposition: "denied",
    });
    assert!(
        denied.outcome.errors.is_empty(),
        "{:?}",
        denied.outcome.errors
    );
    assert_eq!(status.text_content(), "error:true,reject:NotAllowedError");
}

#[test]
fn detached_and_raw_movement_requests_never_claim_native_cursor() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        const detached = document.createElement('div');
        const target = document.body;
        Promise.allSettled([
            detached.requestPointerLock(),
            target.requestPointerLock({ unadjustedMovement: true })
        ]).then(results => document.querySelector('output').textContent =
            results.map(result => result.reason.name).join(','));
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.pointer_lock_actions.is_empty());
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "WrongDocumentError,NotSupportedError"
    );
}
