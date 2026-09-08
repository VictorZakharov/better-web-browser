use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::PresentationAcknowledgement;
use std::time::Duration;

#[test]
fn retained_app_title_events_reach_each_navigation_presentation() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        97,
        include_str!("../fixtures/retained-app-title.html"),
    );
    assert_eq!(initial.title, "Initial item");
    let mut revision = initial.revision;
    for (step, name) in [(1, "First item"), (2, "Second item")] {
        session
            .acknowledge_presentation(PresentationAcknowledgement {
                document: initial.document,
                revision,
                presented: true,
                controls_applied: true,
            })
            .unwrap();
        session
            .advance_time(initial.document, Duration::from_secs(10), 1)
            .unwrap();
        let updated = loop {
            match session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::Presentation(presentation)
                    if presentation.document == initial.document =>
                {
                    break presentation;
                }
                RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
                event => panic!("unexpected title-navigation event: {event:?}"),
            }
        };
        assert_eq!(updated.title, format!("{name} - Retained app"));
        let expected = format!(
            "{step}|{name} - Retained app|/item/{step}|{}|{step}",
            step + 1
        );
        let text = updated
            .layout
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert!(
            text.contains(&expected),
            "expected {expected:?} in {text:?}"
        );
        assert!(updated.revision > revision);
        revision = updated.revision;
    }
    session.shutdown().expect("shutdown renderer");
}
