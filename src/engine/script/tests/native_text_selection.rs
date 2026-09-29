use super::form_user_edits::{native_edit_at, run};
use crate::renderer_protocol::TextSelectionDirection;

#[test]
fn accepted_native_edit_never_mirrors_its_old_value_back_to_the_control() {
    let (dom, mut runtime) = run(r#"<input value='A💡B'><script>
            document.querySelector('input').addEventListener('beforeinput', event => {
                if (event.target.selectionStart !== 4 || event.target.selectionEnd !== 4)
                    throw Error('beforeinput must observe the native pre-edit selection');
                // Even an author setting the same range snapshots the old value.
                event.target.setSelectionRange(4, 4);
            });
        </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit_at(
        &mut runtime,
        field.clone(),
        "A💡BX",
        "insertText",
        Some((4, 4)),
        (5, 5),
    );
    assert!(result.default_allowed);
    assert!(result.rejected_text.is_none());
    assert_eq!(field.input_value(), "A💡BX");
    assert!(result.outcome.selection_actions.is_empty());
}

#[test]
fn author_selection_after_native_edit_mirrors_the_new_value_only() {
    let (dom, mut runtime) = run(r#"<input value='A💡B'><script>
            document.querySelector('input').addEventListener('beforeinput', event => {
                event.target.setSelectionRange(4, 4);
            });
            document.querySelector('input').addEventListener('input', event => {
                event.target.setSelectionRange(1, 3, 'backward');
            });
        </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit_at(
        &mut runtime,
        field.clone(),
        "A💡BX",
        "insertText",
        Some((4, 4)),
        (5, 5),
    );
    assert!(result.default_allowed);
    assert!(result.rejected_text.is_none());
    assert_eq!(field.input_value(), "A💡BX");
    assert_eq!(result.outcome.selection_actions.len(), 1);
    let selection = &result.outcome.selection_actions[0];
    assert_eq!(selection.node, field.id());
    assert_eq!(selection.value, "A💡BX");
    assert_eq!((selection.selection_start, selection.selection_end), (1, 3));
    assert_eq!(selection.direction, TextSelectionDirection::Backward);
}

#[test]
fn canceled_native_replacement_preserves_value_and_utf16_selection_without_echo() {
    let (dom, mut runtime) = run(r#"<input value='A💡B'><script>
            const field = document.querySelector('input');
            field.addEventListener('beforeinput', event => event.preventDefault());
            field.addEventListener('input', () => { throw Error('canceled edit must not commit'); });
        </script>"#);
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit_at(
        &mut runtime,
        field.clone(),
        "AXB",
        "insertText",
        Some((1, 3)),
        (2, 2),
    );
    assert!(!result.default_allowed);
    assert_eq!(field.input_value(), "A💡B");
    let rollback = result.rejected_text.expect("native rollback state");
    assert_eq!(rollback.value, "A💡B");
    assert_eq!((rollback.selection_start, rollback.selection_end), (1, 3));
    assert!(result.outcome.selection_actions.is_empty());
}
