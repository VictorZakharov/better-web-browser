use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::{PresentationAcknowledgement, PresentedViewport};
use std::time::Duration;

#[test]
fn presentation_bursts_keep_the_newest_revision_without_killing_the_renderer() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_inline_document(&session, 93);
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();

    for step in 1..=4 {
        session
            .update_viewport(
                initial.document,
                PresentedViewport {
                    width: 800.0 + step as f32,
                    height: 600.0,
                    style_width: 800.0 + step as f32,
                    dpi: 96,
                },
            )
            .unwrap();
        // The Pong follows the corresponding presentation on the renderer output pipe. Waiting
        // here makes the browser-side backlog deterministic without consuming its events.
        session
            .ping(Duration::from_secs(2))
            .expect("renderer survived presentation burst");
    }

    let mut presentations = Vec::new();
    while let Some(event) = session.try_event().unwrap() {
        match event {
            RendererEvent::Presentation(presentation) => presentations.push(presentation),
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected renderer event after presentation burst: {event:?}"),
        }
    }
    assert_eq!(presentations.len(), 1);
    let newest = presentations.pop().unwrap();
    assert_eq!(newest.revision, initial.revision + 4);
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: newest.document,
            revision: newest.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    session
        .ping(Duration::from_secs(1))
        .expect("renderer accepts acknowledgement that skips coalesced revisions");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().expect("shutdown renderer");
}
