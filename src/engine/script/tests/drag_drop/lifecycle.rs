use super::*;

fn run_drag(html: &str, source_tag: &str, escape: bool) -> String {
    let dom = dom::parse_with_scripting(html, true);
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let source = dom.elements_named(source_tag).next().unwrap();
    let target = dom.elements_named("div").last().unwrap();
    for (phase, node, buttons, x) in [
        ("down", source.clone(), 1, 10.0),
        ("move", target.clone(), 1, 30.0),
    ] {
        pointer(&mut runtime, phase, node, buttons, x);
    }
    if escape {
        let result = runtime.dispatch_user_input(UserInputEvent::Keyboard {
            target: Some(target.clone()),
            phase: "down",
            key: "Escape".into(),
            code: "Escape".into(),
            key_code: 27,
            repeat: false,
            modifiers: UserInputModifiers::default(),
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
    }
    pointer(&mut runtime, "up", target, 0, 30.0);
    dom.elements_named("output").next().unwrap().text_content()
}

fn pointer(
    runtime: &mut ScriptRuntime,
    phase: &'static str,
    node: dom::NodeRef,
    buttons: u8,
    x: f32,
) {
    let result = runtime.dispatch_user_input(UserInputEvent::Pointer {
        phase,
        target: Some(node),
        button: 0,
        buttons,
        x,
        y: 10.0,
        activate: false,
        modifiers: UserInputModifiers::default(),
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
}

#[test]
fn links_offer_resolved_uri_before_dragstart_and_cancel_pointer_stream() {
    let text = run_drag(
        r#"<a href="/destination">drag</a><div>drop</div><output>no</output><script>
            const source = document.querySelector('a'), target = document.querySelector('div');
            const checks = [];
            source.ondragstart = event => {
                checks.push(event.dataTransfer.getData('text/uri-list') ===
                    'https://example.com/destination', event.dataTransfer.dropEffect === 'none');
                event.dataTransfer.effectAllowed = 'link';
            };
            source.onpointercancel = event => checks.push(event.isTrusted && event.buttons === 0);
            target.ondragenter = event => {
                checks.push(event.dataTransfer.getData('text/uri-list') === '');
                event.preventDefault();
            };
            target.ondragover = event => {
                checks.push(event.dataTransfer.dropEffect === 'link');
                event.preventDefault();
            };
            target.ondrop = event => {
                checks.push(event.dataTransfer.getData('text/uri-list') ===
                    'https://example.com/destination');
                event.preventDefault();
            };
            source.ondragend = event => {
                checks.push(event.dataTransfer.dropEffect === 'link');
                document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
            };
        </script>"#,
        "a",
        false,
    );
    assert_eq!(text, "yes");
}

#[test]
fn uncanceled_drop_reports_no_completed_operation() {
    let text = run_drag(
        r#"<img src="/image.png"><div>drop</div><output>no</output><script>
            const source = document.querySelector('img'), target = document.querySelector('div');
            const events = [];
            source.ondragstart = event => {
                events.push(event.dataTransfer.getData('text/uri-list') ===
                    'https://example.com/image.png' ? 'uri' : 'missing');
                event.dataTransfer.effectAllowed = 'copy';
            };
            target.ondragenter = event => event.preventDefault();
            target.ondragover = event => event.preventDefault();
            target.ondrop = () => events.push('drop');
            source.ondragend = event => {
                events.push('end:' + event.dataTransfer.dropEffect);
                document.querySelector('output').textContent = events.join('|');
            };
        </script>"#,
        "img",
        false,
    );
    assert_eq!(text, "uri|drop|end:none");
}

#[test]
fn unsupported_effect_prevents_drop_even_when_dragover_is_canceled() {
    let text = run_drag(
        r#"<div draggable="true">drag</div><div>drop</div><output>no</output><script>
            const source = document.querySelectorAll('div')[0];
            const target = document.querySelectorAll('div')[1];
            source.ondragstart = event => { event.dataTransfer.effectAllowed = 'copy'; };
            target.ondragenter = event => event.preventDefault();
            target.ondragover = event => {
                event.dataTransfer.dropEffect = 'move';
                event.preventDefault();
            };
            target.ondrop = () => document.querySelector('output').textContent = 'unexpected drop';
            source.ondragend = event => document.querySelector('output').textContent =
                'end:' + event.dataTransfer.dropEffect;
        </script>"#,
        "div",
        false,
    );
    assert_eq!(text, "end:none");
}

#[test]
fn uncanceled_dragenter_uses_body_instead_of_the_pointer_target() {
    let text = run_drag(
        r#"<div draggable="true">drag</div><div>drop</div><output>no</output><script>
            const source = document.querySelectorAll('div')[0];
            const target = document.querySelectorAll('div')[1];
            const events = [];
            source.ondragstart = event => { event.dataTransfer.effectAllowed = 'copy'; };
            target.ondragenter = () => events.push('target-enter');
            target.ondragover = () => events.push('unexpected target-over');
            target.ondrop = () => events.push('unexpected target-drop');
            document.body.ondragenter = event => {
                if (event.target === document.body) events.push('body-enter');
            };
            document.body.ondragover = event => {
                if (event.target === document.body) {
                    events.push('body-over');
                    event.preventDefault();
                }
            };
            source.ondragend = event => {
                events.push('end:' + event.dataTransfer.dropEffect);
                document.querySelector('output').textContent = events.join('|');
            };
        </script>"#,
        "div",
        false,
    );
    assert_eq!(text, "target-enter|body-enter|body-over|end:none");
}

#[test]
fn canceling_drag_aborts_without_drop() {
    let text = run_drag(
        r#"<div draggable="true">drag</div><div>drop</div><output>no</output><script>
            const source = document.querySelectorAll('div')[0];
            const target = document.querySelectorAll('div')[1];
            const events = [];
            source.onpointercancel = () => events.push('pointercancel');
            source.ondrag = event => event.preventDefault();
            source.ondragend = event => {
                events.push('end:' + event.dataTransfer.dropEffect);
                document.querySelector('output').textContent = events.join('|');
            };
            target.ondragenter = () => events.push('unexpected enter');
            target.ondrop = () => events.push('unexpected drop');
        </script>"#,
        "div",
        false,
    );
    assert_eq!(text, "pointercancel|end:none");
}

#[test]
fn escape_aborts_drag_and_suppresses_native_keyboard_event() {
    let text = run_drag(
        r#"<div draggable="true">drag</div><div>drop</div><output>no</output><script>
            const source = document.querySelectorAll('div')[0];
            const target = document.querySelectorAll('div')[1];
            const events = [];
            source.ondragstart = event => { event.dataTransfer.effectAllowed = 'copy'; };
            source.onpointercancel = () => events.push('pointercancel');
            source.ondragend = event => {
                events.push('end:' + event.dataTransfer.dropEffect);
                document.querySelector('output').textContent = events.join('|');
            };
            target.ondragenter = event => event.preventDefault();
            target.ondragover = event => event.preventDefault();
            target.ondrop = () => events.push('unexpected drop');
            target.onkeydown = () => events.push('unexpected key');
        </script>"#,
        "div",
        true,
    );
    assert_eq!(text, "pointercancel|end:none");
}
