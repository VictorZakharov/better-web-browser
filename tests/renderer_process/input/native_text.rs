use super::*;
use better_web_browser::renderer_protocol::{NativeTextInput, NativeTextRejection, TextEditIntent};

fn native(
    document: better_web_browser::renderer_protocol::DocumentId,
    target: DocumentNodeId,
    sequence: u64,
    generation: u32,
    value: &str,
) -> DocumentInput {
    let end = value.encode_utf16().count() as u32;
    native_at(
        document,
        target,
        sequence,
        generation,
        value,
        (1, 1),
        (end, end),
    )
}

fn native_at(
    document: better_web_browser::renderer_protocol::DocumentId,
    target: DocumentNodeId,
    sequence: u64,
    generation: u32,
    value: &str,
    pre_selection: (u32, u32),
    post_selection: (u32, u32),
) -> DocumentInput {
    DocumentInput::NativeText(NativeTextInput {
        text: TextInput {
            document,
            sequence,
            target,
            value: value.into(),
            selection_start: post_selection.0,
            selection_end: post_selection.1,
        },
        generation,
        intent: TextEditIntent::InsertText,
        pre_selection: Some(pre_selection),
    })
}

#[test]
fn beforeinput_sees_caret_movement_and_selected_range_before_native_commit() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document(
        &session,
        292,
        r#"<!doctype html>
        <input value=abcd><p id=events>ready</p><script>
            const field = document.querySelector('input');
            const seen = [];
            field.addEventListener('beforeinput', e => seen.push(
                `before:${field.selectionStart}-${field.selectionEnd}:${e.data}`));
            field.addEventListener('input', () => {
                seen.push(`input:${field.selectionStart}-${field.selectionEnd}:${field.value}`);
                document.querySelector('#events').textContent = seen.join('|');
            });
        </script>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    let target = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Text => {
                DocumentNodeId::new(control.node_id.to_wire()).ok()
            }
            _ => None,
        })
        .expect("native input target");

    session
        .send_input(native_at(
            initial.document,
            target,
            1,
            0,
            "abXcd",
            (2, 2),
            (3, 3),
        ))
        .unwrap();
    let moved = wait_for_text(&session, initial.document, "input:3-3:abXcd");
    assert!(presentation_text(&moved).contains("before:2-2:X|input:3-3:abXcd"));

    session
        .send_input(native_at(
            initial.document,
            target,
            2,
            0,
            "aYd",
            (1, 4),
            (2, 2),
        ))
        .unwrap();
    let replaced = wait_for_text(&session, initial.document, "input:2-2:aYd");
    assert!(presentation_text(&replaced).contains("before:1-4:Y|input:2-2:aYd"));
}

fn wait_for_rejection(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
) -> NativeTextRejection {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                if let Some(rejection) = presentation.runtime.native_text_rejection {
                    return rejection;
                }
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                if let Some(rejection) = update.runtime.native_text_rejection {
                    return rejection;
                }
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected native text event: {event:?}"),
        }
    }
}

#[test]
fn canceled_beforeinput_returns_rollback_and_fences_older_queued_edits() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document(
        &session,
        291,
        r#"<!doctype html>
        <input value=a><p id=events>ready</p><script>
            const field = document.querySelector('input');
            let first = true;
            field.addEventListener('beforeinput', e => {
                if (first) { first = false; e.preventDefault(); }
            });
            field.addEventListener('input', () => {
                document.querySelector('#events').textContent = 'input:' + field.value;
            });
        </script>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    let target = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Text => {
                DocumentNodeId::new(control.node_id.to_wire()).ok()
            }
            _ => None,
        })
        .expect("native input target");

    session
        .send_input(native(initial.document, target, 1, 0, "ab"))
        .unwrap();
    let rejected = wait_for_rejection(&session, initial.document);
    assert_eq!(
        (rejected.sequence, rejected.generation, rejected.target),
        (1, 0, target)
    );
    assert_eq!(rejected.value, "a");

    session
        .send_input(native(initial.document, target, 2, 0, "stale"))
        .unwrap();
    session
        .send_input(native(initial.document, target, 3, 1, "ac"))
        .unwrap();
    let accepted = wait_for_text(&session, initial.document, "input:ac");
    assert!(!presentation_text(&accepted).contains("stale"));
    assert!(accepted.runtime.native_text_rejection.is_none());
    assert!(accepted.layout.items.iter().any(|item| matches!(item,
        DisplayItem::Control(control) if control.value == "ac")));
}

#[test]
fn email_native_text_uses_the_tuple_verdict_contract() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document(
        &session,
        293,
        r#"<!doctype html><input type=email value=a><p id=events>ready</p><script>
            const field = document.querySelector('input');
            field.addEventListener('input', event => {
                document.querySelector('#events').textContent =
                    event.inputType + ':' + field.value;
            });
        </script>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    let target = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Text => {
                DocumentNodeId::new(control.node_id.to_wire()).ok()
            }
            _ => None,
        })
        .expect("email input target");
    session
        .send_input(native(initial.document, target, 1, 0, "ab"))
        .unwrap();
    let accepted = wait_for_text(&session, initial.document, "insertText:ab");
    assert!(accepted.runtime.native_text_rejection.is_none());
}

#[test]
fn canceled_beforeinput_rollback_uses_post_job_value_and_generation() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document(
        &session,
        294,
        r#"<!doctype html><input value=a><p id=events>ready</p><script>
            const field = document.querySelector('input');
            let first = true;
            field.addEventListener('beforeinput', event => {
                if (!first) return;
                first = false;
                event.preventDefault();
                Promise.resolve().then(() => {
                    field.value = 'micro';
                    field.setSelectionRange(999, Infinity);
                    document.querySelector('#events').textContent = 'job:' + field.value;
                });
            });
            field.addEventListener('input', () => {
                document.querySelector('#events').textContent = 'input:' + field.value;
            });
        </script>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    let target = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Text => {
                DocumentNodeId::new(control.node_id.to_wire()).ok()
            }
            _ => None,
        })
        .expect("native input target");
    session
        .send_input(native(initial.document, target, 1, 0, "ab"))
        .unwrap();
    let rejected = wait_for_rejection(&session, initial.document);
    assert_eq!((rejected.sequence, rejected.generation), (1, 0));
    assert_eq!(rejected.value, "micro");
    assert_eq!((rejected.selection_start, rejected.selection_end), (5, 5));

    session
        .send_input(native(initial.document, target, 2, 0, "stale"))
        .unwrap();
    session
        .send_input(native_at(
            initial.document,
            target,
            3,
            1,
            "microX",
            (5, 5),
            (6, 6),
        ))
        .unwrap();
    let accepted = wait_for_text(&session, initial.document, "input:microX");
    assert!(!presentation_text(&accepted).contains("stale"));
    assert!(accepted.runtime.native_text_rejection.is_none());
}
