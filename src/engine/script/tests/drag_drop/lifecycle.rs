use super::*;

fn run_drag(html: &str, source_tag: &str, escape: bool) -> String {
    run_drag_with_followup(html, source_tag, escape, None)
}

fn run_drag_with_followup(
    html: &str,
    source_tag: &str,
    escape: bool,
    followup: Option<&str>,
) -> String {
    let dom = dom::parse_with_scripting(html, true);
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script.clone(),
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
    if let Some(code) = followup {
        let result = runtime.execute_additional_with_loader(
            &[ScriptInput {
                source_url: "https://example.com/".into(),
                code: code.into(),
                node: script,
                kind: ScriptKind::Classic,
                fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
                finish_lifecycle: false,
            }],
            None,
        );
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }
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

#[test]
fn trusted_drag_events_have_fresh_transfers_and_detach_after_dispatch() {
    let text = run_drag_with_followup(
        r#"<div draggable=true>drag</div><div>drop</div><output>no</output><script>
            const source = document.querySelectorAll('div')[0];
            const target = document.querySelectorAll('div')[1];
            const checks = [];
            const transfers = [];
            let start, startItem, over, drop, end;
            source.ondragstart = event => {
                start = event.dataTransfer;
                transfers.push(start);
                start.effectAllowed = 'copy';
                start.setData('text/plain', 'payload');
                startItem = start.items[0];
                checks.push(start.items === start.items, startItem === start.items[0],
                    start.getData('text/plain') === 'payload', start.dropEffect === 'none');
            };
            source.ondrag = event => {
                transfers.push(event.dataTransfer);
                checks.push(event.dataTransfer !== start,
                    event.dataTransfer.getData('text/plain') === '',
                    event.dataTransfer.effectAllowed === 'copy');
            };
            target.ondragenter = event => {
                transfers.push(event.dataTransfer);
                event.preventDefault();
            };
            target.ondragover = event => {
                over = event.dataTransfer;
                transfers.push(over);
                checks.push(over !== start, over.items !== start.items,
                    start.items.length === 0, start.items[0] === undefined,
                    start.types.length === 0, startItem.kind === '', startItem.type === '',
                    startItem.getAsFile() === null, startItem.__entry === undefined,
                    over.items.__owner === undefined, over.items.length === 1,
                    over.items[0] === over.items.item(0), over.items[0] !== startItem,
                    over.items[0].kind === 'string', over.getData('text/plain') === '',
                    over.items.add('blocked', 'text/html') === null);
                let invalidState = false;
                try { over.items.remove(0); }
                catch (error) { invalidState = error.name === 'InvalidStateError'; }
                checks.push(invalidState);
                over.effectAllowed = 'move';
                checks.push(over.effectAllowed === 'copy');
                over.dropEffect = 'copy';
                event.preventDefault();
            };
            target.ondrop = event => {
                drop = event.dataTransfer;
                transfers.push(drop);
                checks.push(drop !== over, over.items.length === 0,
                    drop.getData('text/plain') === 'payload', drop.items.length === 1,
                    drop.items.add('blocked', 'text/html') === null,
                    drop.effectAllowed === 'copy', drop.dropEffect === 'copy');
                event.preventDefault();
            };
            source.ondragend = event => {
                end = event.dataTransfer;
                transfers.push(end);
                checks.push(end !== drop, drop.items.length === 0,
                    drop.getData('text/plain') === '', end.dropEffect === 'copy',
                    end.effectAllowed === 'copy', end.getData('text/plain') === '');
            };
        </script>"#,
        "div",
        false,
        Some(
            "document.querySelector('output').textContent = checks.every(Boolean) && new Set(transfers).size === transfers.length && transfers.every(item => item.items.length === 0 && item.types.length === 0 && item.files.length === 0) ? 'yes' : checks.join(',');",
        ),
    );
    assert_eq!(text, "yes");
}

#[test]
fn canceled_dragstart_detaches_its_transfer_and_never_starts_drag() {
    let text = run_drag_with_followup(
        r#"<div draggable=true>drag</div><div>drop</div><output>no</output><script>
            const source = document.querySelectorAll('div')[0];
            const events = [];
            let canceled, item;
            source.ondragstart = event => {
                events.push('start');
                canceled = event.dataTransfer;
                canceled.setData('text/plain', 'secret');
                item = canceled.items[0];
                event.preventDefault();
            };
            source.onpointercancel = () => events.push('pointercancel');
            source.ondrag = () => events.push('drag');
            source.ondragend = () => events.push('dragend');
        </script>"#,
        "div",
        false,
        Some(
            "document.querySelector('output').textContent = [events.join(',') === 'start', canceled.getData('text/plain') === '', canceled.items.length === 0, canceled.types.length === 0, item.kind === '', canceled.items.add('new', 'text/html') === null].every(Boolean) ? 'yes' : events.join(',');",
        ),
    );
    assert_eq!(text, "yes");
}
