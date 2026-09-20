use super::*;
use crate::engine::dom;

// Phase A/B native-path coverage: trusted user edits through
// `dispatch_user_input` exercise user-edit semantics (dirty, user-edited
// length state, number editing buffers, select notifications) that plain
// programmatic sets must not produce.

fn run(html: &str) -> (dom::Dom, ScriptRuntime) {
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
    crate::engine::dom::Node::reset_owned_controls(&form, &dom.document);
    assert_eq!(input.input_value(), "new-default");
    assert_eq!(area.textarea_api_value(), "default");
}
