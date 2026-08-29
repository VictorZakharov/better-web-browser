use super::support::*;
use better_web_browser::engine::{ControlKind, DisplayItem};
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentNodeId, FullscreenAction, FullscreenDisposition, FullscreenResponse,
    InputModifiers, PointerButton, PointerInput, PointerPhase, PresentationAcknowledgement,
};
use std::time::Duration;

#[test]
fn fullscreen_request_and_browser_acknowledgement_cross_the_renderer_boundary() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        119,
        r#"<!doctype html><section id="target">
        <button id="button" type="button">Fullscreen</button><output id="status">idle</output>
        </section><script>
            const target = document.querySelector('#target');
            const status = document.querySelector('#status');
            document.addEventListener('fullscreenchange', () => {
                status.textContent = document.fullscreenElement ? 'entered' : 'exited';
            });
            document.querySelector('#button').addEventListener('click', () => target.requestFullscreen().then(() => {
                status.textContent = [
                    'entered',
                    document.fullscreenElement === target,
                    target.matches(':fullscreen')
                ].join('|');
            }));
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

    let (target, rect) = initial
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
        .expect("fullscreen button target");
    session
        .send_input(DocumentInput::Pointer(PointerInput {
            document: initial.document,
            sequence: 1,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            x: rect.x + rect.width / 2.0,
            y: rect.y + rect.height / 2.0,
            modifiers: InputModifiers::default(),
            target: Some(target),
        }))
        .unwrap();

    let request = loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::FullscreenRequested(request) => break request,
            RendererEvent::Presentation(_) | RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected renderer event before fullscreen request: {event:?}"),
        }
    };
    assert_eq!(request.document, initial.document);
    assert_eq!(request.action, FullscreenAction::Enter);
    session
        .respond_fullscreen(FullscreenResponse {
            document: initial.document,
            request_id: request.request_id,
            disposition: FullscreenDisposition::Entered,
        })
        .unwrap();
    let entered = wait_for_text(&session, initial.document, "entered|true|true");
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: entered.document,
            revision: entered.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();

    session
        .respond_fullscreen(FullscreenResponse {
            document: initial.document,
            request_id: 0,
            disposition: FullscreenDisposition::Exited,
        })
        .unwrap();
    let exited = wait_for_text(&session, initial.document, "exited");
    assert!(!presentation_text(&exited).contains("entered|true|true"));
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: exited.document,
            revision: exited.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();

    session.cancel_document(initial.document).unwrap();
    let replacement = load_inline_document(&session, 120);
    session
        .respond_fullscreen(FullscreenResponse {
            document: initial.document,
            request_id: request.request_id,
            disposition: FullscreenDisposition::Denied,
        })
        .unwrap();
    session
        .ping(Duration::from_secs(1))
        .expect("stale fullscreen response did not wedge renderer");
    assert_eq!(replacement.document.get(), 120);
    assert_eq!(session.snapshot().state, RendererState::Running);
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
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected renderer fullscreen event: {event:?}"),
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
