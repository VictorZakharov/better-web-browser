use super::*;
use crate::engine::dom;

// Phase C: reset semantics, submit gating, reentrancy, change-on-commit,
// and interactive reporting (focus plus browser-owned feedback state).

fn check(html: &str) -> (dom::Dom, ScriptOutcome) {
    let (dom, outcome) = execute_html(html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, outcome)
}

#[test]
fn blocked_submit_reports_focuses_and_sends_nothing() {
    let (dom, outcome) = check(
        r#"<body><form action="/echo"><input name=q required></form><output></output><script>
            const form = document.querySelector('form');
            const field = document.querySelector('input');
            const log = [];
            field.addEventListener('invalid', event => log.push(
                'invalid:' + event.isTrusted + ':' + event.bubbles + ':' + event.cancelable));
            form.addEventListener('submit', () => log.push('submit'));
            form.requestSubmit();
            log.push('active=' + (document.activeElement === field));
            log.push('still-invalid=' + field.validity.valid);
            log.push('message=' + field.validationMessage);
            document.querySelector('output').textContent = log.join('|');
        </script></body>"#,
    );
    assert!(outcome.navigation_url.is_none());
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "invalid:true:false:true|active=true|still-invalid=false|message=Please fill out this field."
    );
}

#[test]
fn novalidate_formnovalidate_and_direct_submit_skip_gating() {
    let (dom, outcome) = execute_html(
        r#"<form id=a action="/echo"><input name=q required></form>
        <form id=b action="/echo" novalidate><input name=q required></form>
        <form id=c action="/echo"><input name=q required><input type=submit formnovalidate></form>
        <output></output><script>
            const logs = [];
            for (const form of document.querySelectorAll('form')) {
                form.addEventListener('invalid', () => logs.push('invalid'));
                form.addEventListener('submit', event => { logs.push('submit'); event.preventDefault(); });
            }
            document.getElementById('a').submit();
            document.getElementById('b').requestSubmit();
            const c = document.getElementById('c');
            c.requestSubmit(c.querySelector('[type=submit]'));
            document.querySelector('output').textContent = logs.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    // Direct submit skips validation and the submit event but still navigates
    // with current values; novalidate paths dispatch submit (prevented here)
    // without any invalid events.
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "submit,submit"
    );
    assert_eq!(
        outcome.navigation_url.as_deref(),
        Some("https://example.com/echo?q=")
    );
}

#[test]
fn canceled_reset_keeps_state_without_spurious_events() {
    let (dom, _) = check(
        r#"<body><form><input value="a"><input type=checkbox checked></form><output></output><script>
            const form = document.querySelector('form');
            const [text, box] = form.querySelectorAll('input');
            const log = [];
            text.value = 'edited';
            box.checked = false;
            for (const name of ['input', 'change', 'reset']) {
                form.addEventListener(name, () => log.push(name));
                text.addEventListener(name, () => log.push('text-' + name));
            }
            form.addEventListener('reset', event => event.preventDefault());
            form.reset();
            log.push(text.value, String(box.checked));
            document.querySelector('output').textContent = log.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "reset|edited|false"
    );
}

#[test]
fn form_validation_ignores_overridden_control_methods() {
    let (dom, _) = check(
        r#"<body><form><input required></form><output></output><script>
            const form = document.querySelector('form');
            const field = document.querySelector('input');
            let invalids = 0;
            field.addEventListener('invalid', () => invalids++);
            field.checkValidity = () => true;
            field.reportValidity = () => true;
            const checked = form.checkValidity();
            const reported = form.reportValidity();
            document.querySelector('output').textContent =
                [checked, reported, invalids, document.activeElement === field].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false|false|2|true"
    );
}

#[test]
fn reentrant_submit_handler_cannot_double_submit() {
    let (_, outcome) = execute_html(
        r#"<form action="/echo"><input name=a value=1><input name=b value=2></form><script>
            const form = document.querySelector('form');
            let submits = 0;
            form.addEventListener('submit', event => {
                submits++;
                // Removing a later control mid-submit must not panic or loop.
                form.querySelector('[name=b]').remove();
                if (submits === 1) form.requestSubmit();
                if (submits > 1) event.preventDefault();
                else event.preventDefault();
            });
            form.requestSubmit();
            document.title = 'submits=' + submits;
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.navigation_url.is_none());
}

#[test]
fn native_blur_commits_user_edits_as_change() {
    let dom = dom::parse_with_scripting(
        r#"<input id=a><input id=b><output></output><script>
            const [first, second] = document.querySelectorAll('input');
            const out = document.querySelector('output');
            for (const [name, field] of [['a', first], ['b', second]]) {
                field.addEventListener('input', () => { out.textContent += name + '-input;'; });
                field.addEventListener('change', event => {
                    out.textContent += name + '-change:' + event.isTrusted + ':' + event.bubbles + ';';
                });
            }
        </script>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let result = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let first = dom.elements_named("input").next().unwrap();
    let second = dom.elements_named("input").nth(1).unwrap();
    let dispatch = |runtime: &mut ScriptRuntime, event: UserInputEvent| {
        let result = runtime.dispatch_user_input(event);
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
        result
    };
    dispatch(
        &mut runtime,
        UserInputEvent::Focus {
            target: Some(first.clone()),
            focused: true,
        },
    );
    dispatch(
        &mut runtime,
        UserInputEvent::Text {
            target: first.clone(),
            value: "typed".into(),
            selection_start: 5,
            selection_end: 5,
        },
    );
    // A programmatic write while focused does not suppress the user commit.
    first.set_input_value("typed-plus");
    dispatch(
        &mut runtime,
        UserInputEvent::Focus {
            target: Some(second.clone()),
            focused: true,
        },
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "a-input;a-change:true:true;"
    );
    // Blurring an untouched control fires no change.
    dispatch(
        &mut runtime,
        UserInputEvent::Focus {
            target: None,
            focused: false,
        },
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "a-input;a-change:true:true;"
    );
}
