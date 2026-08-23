use super::support::*;
use better_web_browser::engine::{ControlKind, DisplayItem};
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentLifecycle, DocumentNodeId, FocusInput, InputModifiers, KeyPhase,
    KeyboardInput, LifecycleInput, NavigationCause, NavigationDisposition, PointerButton,
    PointerInput, PointerPhase, PresentationAcknowledgement, ScrollInput, TextInput,
};
use std::time::Duration;

#[test]
fn native_input_lifecycle_and_navigation_cross_the_real_renderer_boundary() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        101,
        r#"<!doctype html><title>input</title>
        <form action="/submit"><input id="field" name="q" value="start">
        <button type="submit" name="send" value="yes">Submit</button></form>
        <a id="next" href="/next">Next page</a>
        <p id="events">ready</p>
        <script>
            const field = document.querySelector('#field');
            const output = document.querySelector('#events');
            const seen = [];
            const record = value => { seen.push(value); output.textContent = seen.join('|'); };
            field.addEventListener('focus', () => record('focus'));
            field.addEventListener('input', () => record('input:' + field.value));
            field.addEventListener('keydown', event => record('key:' + event.key));
            const submit = document.querySelector('button');
            for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click']) {
                submit.addEventListener(type, () => record('button:' + type));
            }
            const next = document.querySelector('#next');
            next.addEventListener('mouseup', event => event.preventDefault());
            next.addEventListener('contextmenu', event => {
                event.preventDefault();
                record('link:contextmenu');
            });
            document.addEventListener('scroll', () => record('scroll:' + window.scrollY));
            document.addEventListener('visibilitychange', () => record('visibility:' + document.visibilityState));
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
            DisplayItem::Control(control)
                if control.kind == ControlKind::Text && control.value == "start" =>
            {
                DocumentNodeId::new(control.node_id.to_wire()).ok()
            }
            _ => None,
        })
        .expect("input control target");
    session
        .send_input(DocumentInput::Focus(FocusInput {
            document: initial.document,
            sequence: 1,
            focused: true,
            target: Some(target),
        }))
        .unwrap();
    session
        .send_input(DocumentInput::Text(TextInput {
            document: initial.document,
            sequence: 2,
            target,
            value: "changed".into(),
            selection_start: 7,
            selection_end: 7,
        }))
        .unwrap();
    session
        .send_input(DocumentInput::Keyboard(KeyboardInput {
            document: initial.document,
            sequence: 3,
            phase: KeyPhase::Down,
            key: "a".into(),
            code: "KeyA".into(),
            repeat: false,
            modifiers: InputModifiers::default(),
            target: Some(target),
        }))
        .unwrap();
    session
        .send_input(DocumentInput::Scroll(ScrollInput {
            document: initial.document,
            sequence: 4,
            x: 0.0,
            y: 42.0,
        }))
        .unwrap();
    session
        .send_input(DocumentInput::Lifecycle(LifecycleInput {
            document: initial.document,
            sequence: 5,
            state: DocumentLifecycle::Hidden,
        }))
        .unwrap();

    let updated = wait_for_text(&session, initial.document, "visibility:hidden");
    let text = presentation_text(&updated);
    assert!(
        text.contains("focus|input:changed|key:a|scroll:42|visibility:hidden"),
        "{text}"
    );
    let changed_control = updated.layout.items.iter().find_map(|item| match item {
        DisplayItem::Control(control) if control.kind == ControlKind::Text => Some(control),
        _ => None,
    });
    assert_eq!(
        changed_control.map(|control| control.value.as_str()),
        Some("changed")
    );

    session
        .send_input(DocumentInput::Keyboard(KeyboardInput {
            document: initial.document,
            sequence: 6,
            phase: KeyPhase::Down,
            key: "Enter".into(),
            code: "Enter".into(),
            repeat: false,
            modifiers: InputModifiers::default(),
            target: Some(target),
        }))
        .unwrap();
    let (form_url, form_disposition, form_cause) = wait_for_navigation(&session, initial.document);
    assert_eq!(form_url, "https://example.test/submit?q=changed&send=yes");
    assert_eq!(form_disposition, NavigationDisposition::CurrentTab);
    assert_eq!(form_cause, NavigationCause::UserActivation);

    let link = updated
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text {
                rect,
                link: Some(url),
                ..
            } if url.ends_with("/next") => Some((*rect, url.clone())),
            _ => None,
        })
        .expect("renderer link geometry");
    let x = link.0.x + link.0.width / 2.0;
    let y = link.0.y + link.0.height / 2.0;
    for (sequence, phase) in [(7, PointerPhase::Down), (8, PointerPhase::Up)] {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence,
                phase,
                button: PointerButton::Primary,
                x,
                y,
                modifiers: InputModifiers {
                    control: true,
                    ..InputModifiers::default()
                },
                target: None,
            }))
            .unwrap();
    }
    let (url, disposition, cause) = wait_for_navigation(&session, initial.document);
    assert_eq!(url, "https://example.test/next");
    assert_eq!(disposition, NavigationDisposition::NewBackgroundTab);
    assert_eq!(cause, NavigationCause::UserActivation);

    for (sequence, phase) in [(9, PointerPhase::Down), (10, PointerPhase::Up)] {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence,
                phase,
                button: PointerButton::Secondary,
                x,
                y,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .unwrap();
    }
    let context_menu = wait_for_text(&session, initial.document, "link:contextmenu");
    assert!(presentation_text(&context_menu).contains("link:contextmenu"));
    assert!(
        session.wait_for_event(Duration::from_millis(150)).is_err(),
        "secondary link activation unexpectedly requested navigation"
    );

    let submit = updated
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Submit => {
                DocumentNodeId::new(control.node_id.to_wire())
                    .ok()
                    .map(|target| (target, control.rect))
            }
            _ => None,
        })
        .expect("submit control target");
    session
        .send_input(DocumentInput::Pointer(PointerInput {
            document: initial.document,
            sequence: 11,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            x: submit.1.x + submit.1.width / 2.0,
            y: submit.1.y + submit.1.height / 2.0,
            modifiers: InputModifiers::default(),
            target: Some(submit.0),
        }))
        .unwrap();
    let activated = wait_for_text(&session, initial.document, "button:click");
    let activated_text = presentation_text(&activated);
    assert!(
        activated_text.contains(
            "button:pointerdown|button:mousedown|button:pointerup|button:mouseup|button:click"
        ),
        "{activated_text}"
    );
    let (button_url, button_disposition, button_cause) =
        wait_for_navigation(&session, initial.document);
    assert_eq!(button_url, "https://example.test/submit?q=changed&send=yes");
    assert_eq!(button_disposition, NavigationDisposition::CurrentTab);
    assert_eq!(button_cause, NavigationCause::UserActivation);

    session.cancel_document(initial.document).unwrap();
    let replacement = load_html_document(
        &session,
        102,
        r#"<!doctype html><input id="replacement" value="replacement">
        <p id="replacement-output">replacement-ready</p>
        <script>
            document.querySelector('#replacement').addEventListener('input', event => {
                document.querySelector('#replacement-output').textContent = event.target.value;
            });
        </script>"#,
    );
    session
        .send_input(DocumentInput::Text(TextInput {
            document: initial.document,
            sequence: 12,
            target,
            value: "stale-document".into(),
            selection_start: 14,
            selection_end: 14,
        }))
        .unwrap();
    let replacement_target = replacement
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) => DocumentNodeId::new(control.node_id.to_wire()).ok(),
            _ => None,
        })
        .expect("replacement target");
    session
        .send_input(DocumentInput::Text(TextInput {
            document: replacement.document,
            sequence: 1,
            target: replacement_target,
            value: "fresh".into(),
            selection_start: 5,
            selection_end: 5,
        }))
        .unwrap();
    session
        .send_input(DocumentInput::Text(TextInput {
            document: replacement.document,
            sequence: 1,
            target: replacement_target,
            value: "stale-sequence".into(),
            selection_start: 14,
            selection_end: 14,
        }))
        .unwrap();
    let fresh = wait_for_text(&session, replacement.document, "fresh");
    assert!(!presentation_text(&fresh).contains("stale-document"));
    assert!(!presentation_text(&fresh).contains("stale-sequence"));
    session
        .ping(Duration::from_secs(1))
        .expect("renderer remains responsive");
    session.shutdown().expect("shutdown renderer");
}

fn wait_for_text(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    expected: &str,
) -> better_web_browser::renderer_protocol::RendererPresentation {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                if presentation_text(&presentation).contains(expected) {
                    return *presentation;
                }
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::TimeAdvanced { .. } => {}
            event => panic!("unexpected renderer input event: {event:?}"),
        }
    }
}

fn wait_for_navigation(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
) -> (String, NavigationDisposition, NavigationCause) {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::NavigationRequested {
                document: event_document,
                url,
                disposition,
                cause,
            } if event_document == document => return (url, disposition, cause),
            RendererEvent::Presentation(_) | RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected renderer navigation event: {event:?}"),
        }
    }
}

fn presentation_text(
    presentation: &better_web_browser::renderer_protocol::RendererPresentation,
) -> String {
    presentation
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
