use super::*;

#[test]
fn text_selection_applies_only_to_textual_inputs_and_uses_utf16_offsets() {
    let (dom, outcome) = execute_html(
        r#"<body><input id=text value="A💡B"><input id=file type=file>
        <input id=email type=email><input id=number type=number><output></output><script>
            const text = document.getElementById('text');
            const first = [text.selectionStart, text.selectionEnd, text.selectionDirection];
            text.setSelectionRange(1, 3, 'backward');
            const astral = [text.selectionStart, text.selectionEnd, text.selectionDirection];
            text.selectionStart = 4;
            const moved = [text.selectionStart, text.selectionEnd];
            text.setSelectionRange(4, 2);
            const collapsed = [text.selectionStart, text.selectionEnd];
            const unsupported = ['file', 'email', 'number'].map(id => {
                const input = document.getElementById(id);
                const getters = input.selectionStart === null && input.selectionEnd === null &&
                    input.selectionDirection === null;
                const failures = [];
                for (const action of [() => input.selectionStart = 1,
                    () => input.selectionEnd = 1, () => input.selectionDirection = 'backward',
                    () => input.setSelectionRange(0, 1), () => input.setRangeText('x')]) {
                    try { action(); failures.push(false); }
                    catch (error) { failures.push(error.name === 'InvalidStateError'); }
                }
                input.select();
                return getters && failures.every(Boolean);
            });
            document.querySelector('output').textContent = JSON.stringify([
                first, astral, moved, collapsed, unsupported]);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        r#"[[0,0,"none"],[1,3,"backward"],[4,4],[2,2],[true,true,true]]"#
    );
    assert_eq!(outcome.selection_actions.len(), 1);
    let action = &outcome.selection_actions[0];
    assert_eq!((action.selection_start, action.selection_end), (2, 2));
    assert_eq!(
        action.direction,
        crate::renderer_protocol::TextSelectionDirection::None
    );
}

#[test]
fn detached_text_selection_is_independent_of_native_presentation() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const input = document.createElement('input');
            input.setAttribute('value', 'abcd');
            const initial = [input.selectionStart, input.selectionEnd];
            input.setSelectionRange(1, 3, 'backward');
            const selected = [input.selectionStart, input.selectionEnd, input.selectionDirection];
            input.type = 'email';
            const unavailable = input.selectionStart === null;
            input.type = 'text';
            const reset = [input.selectionStart, input.selectionEnd, input.selectionDirection];
            document.querySelector('output').textContent = JSON.stringify([
                initial, selected, unavailable, reset]);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        r#"[[0,0],[1,3,"backward"],true,[0,0,"none"]]"#
    );
    assert_eq!(outcome.selection_actions.len(), 1);
    assert_eq!(
        (
            outcome.selection_actions[0].selection_start,
            outcome.selection_actions[0].selection_end
        ),
        (0, 0)
    );
}

#[test]
fn text_selection_offsets_reject_bigint_without_changing_the_range() {
    let (dom, outcome) = execute_html(
        r#"<body><input value=abcd><output></output><script>
            const input = document.querySelector('input');
            input.setSelectionRange(1, 3, 'backward');
            const failures = [];
            for (const action of [
                () => input.selectionStart = 1n,
                () => input.selectionEnd = 2n,
                () => input.setSelectionRange(1n, 2),
                () => input.setSelectionRange(1, 2n),
                () => input.setRangeText('x', 1n, 2),
                () => input.setRangeText('x', 1, 2n),
                () => input.setSelectionRange({valueOf: () => 1n}, 2),
                () => input.selectionEnd = Symbol('offset')
            ]) {
                try { action(); failures.push(false); }
                catch (error) { failures.push(error instanceof TypeError); }
            }
            document.querySelector('output').textContent = JSON.stringify([
                failures.every(Boolean), input.value, input.selectionStart,
                input.selectionEnd, input.selectionDirection]);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        r#"[true,"abcd",1,3,"backward"]"#
    );
    assert_eq!(outcome.selection_actions.len(), 1);
    let action = &outcome.selection_actions[0];
    assert_eq!((action.selection_start, action.selection_end), (1, 3));
    assert_eq!(
        action.direction,
        crate::renderer_protocol::TextSelectionDirection::Backward
    );
}

#[test]
fn range_replacement_modes_and_value_setters_follow_html_selection_rules() {
    let (dom, outcome) = execute_html(
        r#"<body><input id=text value=abcdef><textarea id=area></textarea>
        <output></output><script>
            const input = document.getElementById('text');
            const area = document.getElementById('area');
            area.value = 'A\r\nB';
            const normalized = area.value === 'A\nB';
            const events = [];
            input.addEventListener('input', () => events.push('input'));
            input.addEventListener('change', () => events.push('change'));
            input.setSelectionRange(2, 4);
            input.setRangeText('XY');
            const preserve = [input.value, input.selectionStart, input.selectionEnd];
            input.setRangeText('Z', 2, 4, 'select');
            const select = [input.value, input.selectionStart, input.selectionEnd];
            input.setRangeText('QQ', 1, 2, 'start');
            const start = [input.value, input.selectionStart, input.selectionEnd];
            input.setRangeText('T', 1, 3, 'end');
            const end = [input.value, input.selectionStart, input.selectionEnd];
            let invalid = false;
            try { input.setRangeText('!', 4, 2); }
            catch (error) { invalid = error.name === 'IndexSizeError'; }
            input.setSelectionRange(1, 3);
            input.value = input.value;
            const unchanged = [input.selectionStart, input.selectionEnd];
            input.value = 'hi';
            const changed = [input.selectionStart, input.selectionEnd];
            input.setSelectionRange(1, 2);
            input.setRangeText('ZZ', 0, 1, 'preserve');
            const explicitPreserve = [input.value, input.selectionStart, input.selectionEnd];
            area.setRangeText('X', 0, 2, 'end');
            const textarea = [area.value, area.selectionStart, area.selectionEnd, area.textLength];
            document.querySelector('output').textContent = JSON.stringify([
                preserve, select, start, end, invalid, unchanged, changed,
                explicitPreserve, normalized, textarea, events]);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        r#"[["abXYef",2,4],["abZef",2,3],["aQQZef",1,1],["aTZef",2,2],true,[1,3],[2,2],["ZZi",1,2],true,["XB",1,1,2],[]]"#
    );
    let input_id = dom.elements_named("input").next().unwrap().id();
    let area_id = dom.elements_named("textarea").next().unwrap().id();
    assert_eq!(
        outcome
            .selection_actions
            .iter()
            .find(|action| action.node == input_id)
            .unwrap()
            .value,
        "ZZi"
    );
    assert_eq!(
        outcome
            .selection_actions
            .iter()
            .find(|action| action.node == area_id)
            .unwrap()
            .value,
        "XB"
    );
}

#[test]
fn form_reset_clamps_text_control_selection_without_input_events() {
    let (dom, outcome) = execute_html(
        r#"<body><form><input value=ab><textarea>xy</textarea></form><output></output><script>
            const form = document.querySelector('form');
            const input = form.querySelector('input');
            const area = form.querySelector('textarea');
            const events = [];
            for (const control of [input, area]) {
                control.addEventListener('input', () => events.push('input'));
                control.addEventListener('change', () => events.push('change'));
            }
            input.value = 'abcdefgh';
            area.value = 'mnopqrst';
            input.setSelectionRange(7, 8);
            area.setSelectionRange(7, 8);
            form.reset();
            document.querySelector('output').textContent = JSON.stringify([
                [input.value, input.selectionStart, input.selectionEnd],
                [area.value, area.selectionStart, area.selectionEnd], events]);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        r#"[["ab",2,2],["xy",2,2],[]]"#
    );
    assert_eq!(outcome.selection_actions.len(), 2);
    assert!(
        outcome
            .selection_actions
            .iter()
            .all(|action| matches!(action.value.as_str(), "ab" | "xy"))
    );
    assert!(
        outcome
            .selection_actions
            .iter()
            .all(|action| (action.selection_start, action.selection_end) == (2, 2))
    );
}

#[test]
fn default_value_changes_clamp_clean_control_selections() {
    let (dom, outcome) = execute_html(
        r#"<body><input value=abcd><textarea>abcd</textarea><output></output><script>
            const input = document.querySelector('input');
            const area = document.querySelector('textarea');
            input.setSelectionRange(3, 4);
            area.setSelectionRange(3, 4);
            input.defaultValue = 'x';
            area.defaultValue = 'x';
            document.querySelector('output').textContent = JSON.stringify([
                [input.value, input.selectionStart, input.selectionEnd],
                [area.value, area.selectionStart, area.selectionEnd]]);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        r#"[["x",1,1],["x",1,1]]"#
    );
    assert_eq!(outcome.selection_actions.len(), 2);
    assert!(
        outcome
            .selection_actions
            .iter()
            .all(|action| action.value == "x")
    );
}

#[test]
fn selection_events_are_queued_trusted_and_do_not_fake_input() {
    let dom = dom::parse_with_scripting(
        r#"<body><input value=abcd><script>
            const input = document.querySelector('input');
            const seen = [];
            for (const name of ['select', 'selectionchange', 'input', 'change'])
                input.addEventListener(name, event => seen.push(name + ':' + event.isTrusted));
            input.setSelectionRange(1, 2);
            input.setSelectionRange(1, 2);
            input.setSelectionRange(2, 3);
            document.body.setAttribute('data-sync', seen.join('|'));
            setTimeout(() => document.body.setAttribute('data-events', seen.join('|')), 0);
        </script></body>"#,
        true,
    );
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&scripts);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-sync").as_deref(), Some(""));
    let delivered = runtime.advance_time(std::time::Duration::ZERO, 16);
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some("selectionchange:true|select:true|select:true")
    );
}

#[test]
fn native_text_edit_updates_selection_before_input_listeners() {
    let dom = dom::parse_with_scripting(
        r#"<body><textarea>A</textarea><script>
            const area = document.querySelector('textarea');
            area.addEventListener('input', () => document.body.setAttribute('data-selection',
                [area.value, area.selectionStart, area.selectionEnd].join('|')));
        </script></body>"#,
        true,
    );
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&scripts);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let result = runtime.dispatch_user_input(UserInputEvent::Text {
        target: dom.elements_named("textarea").next().unwrap(),
        value: "A\nB".into(),
        selection_start: 2,
        selection_end: 2,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert!(
        result.outcome.selection_actions.is_empty(),
        "native updates must not echo to the host"
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-selection")
            .as_deref(),
        Some("A\nB|2|2")
    );
}

#[test]
fn native_selection_only_queues_selection_events_without_edit_or_echo() {
    let dom = dom::parse_with_scripting(
        r#"<body><input value=abcd><script>
            const input = document.querySelector('input');
            const seen = [];
            for (const name of ['selectionchange', 'select', 'input', 'change']) {
                input.addEventListener(name, event => {
                    seen.push(name + ':' + event.isTrusted);
                    document.body.setAttribute('data-events', seen.join('|'));
                    document.body.setAttribute('data-state',
                        [input.value, input.selectionStart, input.selectionEnd,
                            input.selectionDirection].join('|'));
                });
            }
        </script></body>"#,
        true,
    );
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&scripts);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let input = dom.elements_named("input").next().unwrap();
    let result = runtime.dispatch_user_input(UserInputEvent::Selection {
        target: input,
        selection_start: 1,
        selection_end: 3,
        direction: crate::renderer_protocol::TextSelectionDirection::Backward,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert!(
        result.outcome.selection_actions.is_empty(),
        "native selection must not echo"
    );
    let body = dom.elements_named("body").next().unwrap();
    assert!(
        body.attr("data-events").is_none(),
        "selection events should be queued"
    );
    let delivered = runtime.advance_time(std::time::Duration::ZERO, 16);
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    assert!(delivered.selection_actions.is_empty());
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some("selectionchange:true|select:true")
    );
    assert_eq!(
        body.attr("data-state").as_deref(),
        Some("abcd|1|3|backward")
    );
}
