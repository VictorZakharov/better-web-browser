use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentInput, HistoryAction, HistoryTraversalInput, RuntimeReport,
};
use std::time::{Duration, Instant};

#[test]
fn document_scoped_history_input_restores_state_before_ordered_events() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        920,
        r#"<!doctype html><output>initial</output><script>
            const retainedDocument = document;
            const seen = [];
            const publish = () => document.querySelector('output').textContent = seen.join('|');
            addEventListener('popstate', event => {
                seen.push(['pop', event.state?.step, history.state?.step,
                    location.href, document.URL, history.length,
                    document === retainedDocument, event.isTrusted].join(':'));
                publish();
            });
            addEventListener('hashchange', event => {
                seen.push(['hash', event.oldURL, event.newURL, event.isTrusted].join(':'));
                publish();
            });
        </script>"#,
    );
    acknowledge(&session, &initial);
    let before = format!("https://example.test/{}", initial.document.get());
    let after = format!("{before}#step");
    session
        .send_input(DocumentInput::History(HistoryTraversalInput {
            document: initial.document,
            sequence: 1,
            url: after.clone(),
            state: Some(r#"{"t":"object","id":1,"n":false,"v":[["step",1]]}"#.into()),
            history_length: 3,
            history_index: 1,
        }))
        .expect("send same-document traversal");

    let expected = format!("pop:1:1:{after}:{after}:3:true:true|hash:{before}:{after}:true");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_ack = false;
    loop {
        assert!(
            Instant::now() < deadline,
            "renderer did not dispatch history events"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation)
                if presentation.document == initial.document =>
            {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                if let Some(sequence) = presentation.runtime.history_traversal_ack {
                    assert_eq!(sequence, 1, "acknowledged the wrong history input");
                    saw_ack = true;
                }
                let text = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                if text.contains(&expected) {
                    assert!(saw_ack, "history input had no renderer acknowledgement");
                    break;
                }
                acknowledge(&session, &presentation);
                pump_ready_task(&session, initial.document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == initial.document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                if let Some(sequence) = update.runtime.history_traversal_ack {
                    assert_eq!(sequence, 1, "acknowledged the wrong history input");
                    saw_ack = true;
                }
                pump_ready_task(&session, initial.document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("history traversal replaced or navigated the document: {event:?}"),
        }
    }
    session.shutdown().expect("shutdown renderer");
}

#[test]
fn inline_push_state_and_location_assign_share_the_initial_runtime_report() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let document = DocumentId::new(921).unwrap();
    let html = r#"<!doctype html><script>
        history.pushState({step: 1}, '', '/one');
        location.assign('/two');
    </script>"#;
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html.as_bytes().to_vec(),
        )
        .expect("load inline navigation fixture");
    let one = "https://example.test/one";
    let two = "https://example.test/two";
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "inline navigation report was not delivered"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                if assert_navigation_history_order(&presentation.runtime, one, two) {
                    break;
                }
                acknowledge(&session, &presentation);
                pump_ready_task(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                if assert_navigation_history_order(&update.runtime, one, two) {
                    break;
                }
                pump_ready_task(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("history update was not reported before full navigation: {event:?}"),
        }
    }
    session.shutdown().expect("shutdown renderer");
}

fn assert_navigation_history_order(report: &RuntimeReport, one: &str, two: &str) -> bool {
    assert!(
        report.errors.is_empty(),
        "script errors: {:?}",
        report.errors
    );
    if report.navigation_url.as_deref() != Some(two) {
        return false;
    }
    assert!(
        matches!(
            report.history_actions.as_slice(),
            [HistoryAction::Update { url, replace: false, state: Some(_) }] if url == one
        ),
        "history update was dropped from navigation report: {report:?}"
    );
    true
}
