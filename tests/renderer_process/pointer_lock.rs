use super::support::*;
use better_web_browser::engine::{ControlKind, DisplayItem};
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentNodeId, InputModifiers, PointerButton, PointerInput,
    PointerLockDisposition, PointerLockResponse, PointerPhase, PresentationAcknowledgement,
};
use std::time::Duration;

#[test]
fn pointer_lock_request_relative_motion_and_release_cross_renderer_boundary() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        175,
        r#"<!doctype html><section><button id="lock" type="button">Lock</button><output>idle</output></section><script>
            const lock = document.querySelector('#lock');
            const status = document.querySelector('output');
            document.onpointerlockchange = () => {
                status.textContent = document.pointerLockElement ? 'locked' : 'released';
            };
            lock.onclick = () => lock.requestPointerLock().then(() => {
                status.textContent = document.pointerLockElement === lock ? 'ready' : 'wrong';
            });
            lock.onmousemove = event => {
                if (document.pointerLockElement === lock)
                    status.textContent = 'motion:' + event.movementX + ',' + event.movementY;
            };
        </script>"#,
    );
    acknowledge(&session, &initial);
    let (button, rect) = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Button => {
                DocumentNodeId::new(control.node_id.to_wire())
                    .ok()
                    .map(|target| (target, control.rect))
            }
            _ => None,
        })
        .expect("button target");
    session
        .send_input(DocumentInput::Pointer(PointerInput {
            document: initial.document,
            sequence: 1,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            buttons: 0,
            x: rect.x + rect.width / 2.0,
            y: rect.y + rect.height / 2.0,
            modifiers: InputModifiers::default(),
            target: Some(button),
        }))
        .unwrap();
    let request = loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::PointerLockRequested(request) => break request,
            RendererEvent::Presentation(_)
            | RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected renderer event: {event:?}"),
        }
    };
    assert_eq!(request.document, initial.document);
    assert_eq!(request.target, Some(button));
    session
        .respond_pointer_lock(PointerLockResponse {
            document: initial.document,
            request_id: request.request_id,
            disposition: PointerLockDisposition::Entered,
        })
        .unwrap();
    acknowledge(&session, &wait_for_text(&session, "ready"));

    session
        .send_input(DocumentInput::Pointer(PointerInput {
            document: initial.document,
            sequence: 2,
            phase: PointerPhase::LockedMove,
            button: PointerButton::None,
            buttons: 0,
            x: -5.0,
            y: 9.0,
            modifiers: InputModifiers::default(),
            target: Some(button),
        }))
        .unwrap();
    acknowledge(&session, &wait_for_text(&session, "motion:-5,9"));

    session
        .respond_pointer_lock(PointerLockResponse {
            document: initial.document,
            request_id: 0,
            disposition: PointerLockDisposition::Exited,
        })
        .unwrap();
    acknowledge(&session, &wait_for_text(&session, "released"));

    session.cancel_document(initial.document).unwrap();
    let replacement = load_inline_document(&session, 176);
    session
        .respond_pointer_lock(PointerLockResponse {
            document: initial.document,
            request_id: 0,
            disposition: PointerLockDisposition::Exited,
        })
        .unwrap();
    session
        .ping(Duration::from_secs(1))
        .expect("stale response kept renderer alive");
    assert_eq!(replacement.document.get(), 176);
    session.shutdown().expect("shutdown renderer");
}

fn acknowledge(
    session: &RendererSession,
    presentation: &better_web_browser::renderer_protocol::RendererPresentation,
) {
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: presentation.document,
            revision: presentation.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
}

fn wait_for_text(
    session: &RendererSession,
    expected: &str,
) -> better_web_browser::renderer_protocol::RendererPresentation {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                let text = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if text.contains(expected) {
                    return *presentation;
                }
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected renderer event: {event:?}"),
        }
    }
}
