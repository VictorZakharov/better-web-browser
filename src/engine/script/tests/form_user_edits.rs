use super::*;
use crate::engine::dom;

// Phase A/B native-path coverage: trusted user edits through
// `dispatch_user_input` exercise user-edit semantics (dirty, user-edited
// length state, number editing buffers, select notifications) that plain
// programmatic sets must not produce.

pub(super) fn run(html: &str) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(html, true);
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
    (dom, runtime)
}

fn edit(runtime: &mut ScriptRuntime, target: dom::NodeRef, value: &str) -> UserInputResult {
    let result = runtime.dispatch_user_input(UserInputEvent::Text {
        target,
        value: value.into(),
        selection_start: value.len() as u32,
        selection_end: value.len() as u32,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    result
}

fn native_edit(
    runtime: &mut ScriptRuntime,
    target: dom::NodeRef,
    value: &str,
    input_type: &'static str,
) -> UserInputResult {
    let before = target.input_value().encode_utf16().count() as u32;
    let offset = value.encode_utf16().count() as u32;
    native_edit_at(
        runtime,
        target,
        value,
        input_type,
        Some((before, before)),
        (offset, offset),
    )
}

pub(super) fn native_edit_at(
    runtime: &mut ScriptRuntime,
    target: dom::NodeRef,
    value: &str,
    input_type: &'static str,
    pre_selection: Option<(u32, u32)>,
    post_selection: (u32, u32),
) -> UserInputResult {
    let result = runtime.dispatch_user_input(UserInputEvent::NativeText {
        target,
        value: value.into(),
        selection_start: post_selection.0,
        selection_end: post_selection.1,
        input_type,
        pre_selection,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    result
}

#[test]
fn beforeinput_observes_moved_caret_and_replacement_selection() {
    let (dom, mut runtime) = run(r#"<input value=abcd><output></output><script>
        const field = document.querySelector('input');
        const events = [];
        field.addEventListener('beforeinput', e => {
            events.push(['before', field.selectionStart, field.selectionEnd,
                e.data, e.getTargetRanges().length].join(':'));
        });
        field.addEventListener('input', e => {
            events.push(['input', field.selectionStart, field.selectionEnd, field.value, e.data].join(':'));
            document.querySelector('output').textContent = events.join('|');
        });
    </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit_at(
        &mut runtime,
        field.clone(),
        "abXcd",
        "insertText",
        Some((2, 2)),
        (3, 3),
    );
    assert!(result.default_allowed);
    assert_eq!(output_text(&dom), "before:2:2:X:0|input:3:3:abXcd:X");

    let result = native_edit_at(
        &mut runtime,
        field.clone(),
        "aYd",
        "insertText",
        Some((1, 4)),
        (2, 2),
    );
    assert!(result.default_allowed);
    assert_eq!(
        output_text(&dom),
        "before:2:2:X:0|input:3:3:abXcd:X|before:1:4:Y:0|input:2:2:aYd:Y"
    );
}

#[test]
fn selected_range_replacement_reports_inserted_data_without_common_suffix_confusion() {
    let (dom, mut runtime) = run(r#"<input value=foo><output></output><script>
        const field = document.querySelector('input');
        field.addEventListener('beforeinput', e => {
            document.querySelector('output').textContent =
                [field.selectionStart, field.selectionEnd, e.data].join(':');
        });
    </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit_at(
        &mut runtime,
        field.clone(),
        "o",
        "insertText",
        Some((0, 3)),
        (1, 1),
    );
    assert!(result.default_allowed);
    assert_eq!(output_text(&dom), "0:3:o");
}

#[test]
fn missing_pre_edit_selection_cannot_dispatch_misleading_beforeinput() {
    let (dom, mut runtime) = run(r#"<input value=ab><output></output><script>
        const field = document.querySelector('input');
        field.addEventListener('beforeinput', () => {
            document.querySelector('output').textContent = 'unexpected';
        });
    </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit_at(
        &mut runtime,
        field.clone(),
        "abc",
        "insertText",
        None,
        (3, 3),
    );
    assert!(!result.default_allowed);
    assert_eq!(result.rejected_text.unwrap().value, "ab");
    assert_eq!(field.input_value(), "ab");
    assert_eq!(output_text(&dom), "");
}

fn output_text(dom: &dom::Dom) -> String {
    dom.elements_named("output").next().unwrap().text_content()
}

#[test]
fn native_edits_drive_length_flags_that_scripts_cannot() {
    let (dom, mut runtime) = run(
        r#"<input maxlength="3" minlength="2"><output></output><script>
            const field = document.querySelector('input');
            field.addEventListener('input', () => {
                document.querySelector('output').textContent =
                    [field.validity.tooLong, field.validity.tooShort, field.validity.valid].join('|');
            });
        </script>"#,
    );
    let field = dom.elements_named("input").next().unwrap();
    edit(&mut runtime, field.clone(), "toolong");
    assert_eq!(output_text(&dom), "true|false|false");
    // A script-written twin value is not a user edit: no length flags.
    field.set_input_value("toolong");
    assert_eq!(output_text(&dom), "true|false|false");
    drop(runtime);
    let (dom, _) = execute_html(
        r#"<body><input maxlength="3" minlength="2"><output></output><script>
            const field = document.querySelector('input');
            field.value = 'toolong';
            document.querySelector('output').textContent =
                [field.validity.tooLong, field.validity.tooShort, field.validity.valid].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false|false|true"
    );
}

#[test]
fn native_number_edits_keep_bad_input_until_valid() {
    let (dom, mut runtime) = run(r#"<input type=number><output></output><script>
            const field = document.querySelector('input');
            field.addEventListener('input', () => {
                document.querySelector('output').textContent =
                    [JSON.stringify(field.value), field.validity.badInput, field.validity.valid].join('|');
            });
        </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    edit(&mut runtime, field.clone(), "abc");
    assert_eq!(output_text(&dom), "\"\"|true|false");
    assert_eq!(field.input_value(), "");
    edit(&mut runtime, field.clone(), "12");
    assert_eq!(output_text(&dom), "\"12\"|false|true");
}

#[test]
fn native_select_pick_notifies_and_marks_user_validity() {
    let (dom, mut runtime) = run(
        r#"<body><select><option value=a>A</option><option value=b>B</option></select><output></output><script>
            const select = document.querySelector('select');
            const log = [];
            select.addEventListener('input', () => log.push('input'));
            select.addEventListener('change', () => {
                log.push('change');
                document.querySelector('output').textContent =
                    log.join(',') + '|' + select.value;
            });
        </script></body>"#,
    );
    let select = dom.elements_named("select").next().unwrap();
    edit(&mut runtime, select.clone(), "b");
    assert_eq!(output_text(&dom), "input,change|b");
    assert!(select.control_state_snapshot().user_validity);
}

#[test]
fn native_edits_keep_reset_defaults_intact() {
    let (dom, mut runtime) = run(
        r#"<body><form><input value="a"><textarea>default</textarea></form><output></output><script>
            document.querySelector('form').addEventListener('reset', () => {});
        </script></body>"#,
    );
    let input = dom.elements_named("input").next().unwrap();
    let area = dom.elements_named("textarea").next().unwrap();
    edit(&mut runtime, input.clone(), "edited");
    edit(&mut runtime, area.clone(), "changed");
    assert_eq!(input.input_value(), "edited");
    // Defaults still follow attributes while dirty edits are kept.
    input.set_attr("value", "new-default");
    assert_eq!(input.input_value(), "edited");
    assert_eq!(input.input_default_value(), "new-default");
    let form = dom.elements_named("form").next().unwrap();
    crate::engine::dom::node::control_reset::reset_owned_controls(&form, &dom.document);
    assert_eq!(input.input_value(), "new-default");
    assert_eq!(area.textarea_api_value(), "default");
}

#[test]
fn ordinary_native_edit_dispatches_trusted_beforeinput_before_commit_and_input_after() {
    let (dom, mut runtime) = run(r#"<input value=ab><output></output><script>
        const field = document.querySelector('input');
        const events = [];
        field.addEventListener('beforeinput', e => events.push(
            ['before', field.value, e.inputType, e.data, e.isTrusted, e.cancelable].join(':')));
        field.addEventListener('input', e => {
            events.push(['input', field.value, e.inputType, e.data, e.isTrusted, e.cancelable].join(':'));
            document.querySelector('output').textContent = events.join('|');
        });
    </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit(&mut runtime, field.clone(), "abc", "insertText");
    assert!(result.default_allowed);
    assert!(result.rejected_text.is_none());
    assert!(
        result.outcome.selection_actions.is_empty(),
        "native ingress must not echo a stale pre-edit value"
    );
    assert_eq!(field.input_value(), "abc");
    assert_eq!(
        output_text(&dom),
        "before:ab:insertText:c:true:true|input:abc:insertText:c:true:false"
    );
}

#[test]
fn canceled_native_edit_preserves_authoritative_value_and_selection() {
    let (dom, mut runtime) = run(r#"<input value=abc><output></output><script>
        const field = document.querySelector('input');
        field.addEventListener('beforeinput', e => {
            document.querySelector('output').textContent =
                [field.value, e.inputType, e.data, e.isTrusted].join('|');
            field.setSelectionRange(1, 1);
            e.preventDefault();
        });
        field.addEventListener('input', () => { throw Error('input must not fire'); });
    </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit(&mut runtime, field.clone(), "ab", "deleteContentBackward");
    assert!(!result.default_allowed);
    assert_eq!(output_text(&dom), "abc|deleteContentBackward||true");
    assert_eq!(field.input_value(), "abc");
    let rollback = result
        .rejected_text
        .expect("canceled edit returns Win32 rollback");
    assert_eq!(
        (
            rollback.value.as_str(),
            rollback.selection_start,
            rollback.selection_end
        ),
        ("abc", 1, 1)
    );
}

#[test]
fn canceled_native_edit_rolls_back_to_post_microtask_value_and_clamped_selection() {
    let (dom, mut runtime) = run(r#"<input value=abc><output></output><script>
        const field = document.querySelector('input');
        field.addEventListener('beforeinput', event => {
            event.preventDefault();
            Promise.resolve().then(() => {
                field.value = 'micro';
                field.setSelectionRange(999, 999);
                document.querySelector('output').textContent =
                    [field.value, field.selectionStart, field.selectionEnd].join(':');
            });
        });
        field.addEventListener('input', () => { throw Error('input must not fire'); });
    </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit(&mut runtime, field.clone(), "ab", "deleteContentBackward");
    assert!(!result.default_allowed);
    assert_eq!(field.input_value(), "micro");
    assert_eq!(output_text(&dom), "micro:5:5");
    let rollback = result.rejected_text.expect("post-job rollback state");
    assert_eq!(rollback.value, "micro");
    assert_eq!((rollback.selection_start, rollback.selection_end), (5, 5));
}

#[test]
fn native_text_for_other_textual_input_types_returns_a_valid_verdict() {
    let (dom, mut runtime) = run(r#"<input type=email><input type=url><input type=tel>
        <input type=number><output></output><script>
        for (const field of document.querySelectorAll('input')) {
            field.addEventListener('beforeinput', () => { throw Error('not an ordinary edit'); });
            field.addEventListener('input', event => {
                document.querySelector('output').textContent +=
                    field.type + ':' + event.inputType + '|';
            });
        }
    </script>"#);
    for field in dom.elements_named("input") {
        let result = native_edit(&mut runtime, field.clone(), "1", "insertText");
        assert!(result.default_allowed);
        assert!(result.rejected_text.is_none());
        assert_eq!(field.input_value(), "1");
    }
    assert_eq!(
        output_text(&dom),
        "email:insertText|url:insertText|tel:insertText|number:insertText|"
    );
}

#[test]
fn native_text_during_pointer_drag_returns_a_rollback_verdict() {
    let (dom, mut runtime) = run(r#"<div draggable=true>drag</div><input value=abc>
        <output></output><script>
        document.querySelector('input').addEventListener('input',
            () => { throw Error('device input is suppressed during a drag'); });
    </script>"#);
    let source = dom.elements_named("div").next().unwrap();
    let field = dom.elements_named("input").next().unwrap();
    for (phase, target, x) in [("down", source, 10.0), ("move", field.clone(), 30.0)] {
        let result = runtime.dispatch_user_input(UserInputEvent::Pointer {
            target: Some(target),
            phase,
            button: 0,
            buttons: 1,
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
    let result = native_edit(&mut runtime, field.clone(), "abcd", "insertText");
    assert!(!result.default_allowed);
    assert_eq!(result.rejected_text.expect("drag rollback").value, "abc");
    assert_eq!(field.input_value(), "abc");
}

#[test]
fn set_selection_range_clamps_out_of_range_offsets() {
    let (dom, outcome) = execute_html(
        r#"<input value=abc><output></output><script>
        const field = document.querySelector('input');
        const offsets = [];
        field.setSelectionRange(999, Infinity);
        offsets.push([field.selectionStart, field.selectionEnd].join(':'));
        field.setSelectionRange(2, 1);
        offsets.push([field.selectionStart, field.selectionEnd].join(':'));
        field.setSelectionRange(-100, Infinity);
        offsets.push([field.selectionStart, field.selectionEnd].join(':'));
        document.querySelector('output').textContent = offsets.join('|');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(output_text(&dom), "0:0|1:1|0:0");
}

#[test]
fn programmatic_write_does_not_fire_native_input_events() {
    let (dom, mut runtime) = run(r#"<input><output></output><script>
        const field = document.querySelector('input');
        const output = document.querySelector('output');
        field.addEventListener('beforeinput', () => output.textContent += 'before');
        field.addEventListener('input', () => output.textContent += 'input');
        field.value = 'script';
    </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    assert_eq!(field.input_value(), "script");
    assert_eq!(output_text(&dom), "");
    native_edit(&mut runtime, field.clone(), "script!", "insertText");
    assert_eq!(output_text(&dom), "beforeinput");
}

#[test]
fn unclassified_native_edit_does_not_claim_ordinary_beforeinput_semantics() {
    let (dom, mut runtime) = run(r#"<input><output></output><script>
        const field = document.querySelector('input');
        field.addEventListener('beforeinput', () => { throw Error('not typed text'); });
        field.addEventListener('input', e => {
            document.querySelector('output').textContent = e.inputType;
        });
    </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    native_edit(&mut runtime, field.clone(), "paste", "");
    assert_eq!(field.input_value(), "paste");
    assert_eq!(output_text(&dom), "");
}
