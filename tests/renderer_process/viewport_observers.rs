use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentInput, PresentationAcknowledgement, RuntimeReport, ScrollInput,
};
use std::time::Duration;

fn next_report(
    session: &RendererSession,
    document: DocumentId,
) -> (bool, bool, Option<u64>, RuntimeReport) {
    loop {
        match session
            .wait_for_event(Duration::from_secs(3))
            .expect("renderer observer event")
        {
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                return (
                    false,
                    update.clock_advanced,
                    update.next_timer_micros,
                    update.runtime,
                );
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                return (
                    true,
                    presentation.clock_advanced,
                    presentation.next_timer_micros,
                    presentation.runtime,
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected observer event: {event:?}"),
        }
    }
}

fn advance(session: &RendererSession, document: DocumentId) -> (bool, RuntimeReport) {
    session.advance_time(document, Duration::ZERO, 1).unwrap();
    let (painted, advanced, _, report) = next_report(session, document);
    assert!(advanced, "expected dedicated observer task completion");
    (painted, report)
}

#[test]
fn native_scroll_queues_observer_work_without_relayout_or_author_event_propagation() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        119,
        r#"<!doctype html>
        <style>body{margin:0} #target{position:absolute;top:1000px;width:100px;height:100px}</style>
        <div id=target></div><script>
            const target = document.getElementById('target');
            new IntersectionObserver(entries => {
                for (const e of entries) console.log('entry:' + e.isIntersecting + ':' + e.boundingClientRect.top);
            }).observe(target);
            document.addEventListener('scroll', event => {
                event.stopPropagation(); event.stopImmediatePropagation();
            });
            document.dispatchEvent(new Event('scroll', {bubbles:true}));
        </script>"#,
    );
    assert!(
        initial.runtime.console.is_empty(),
        "synthetic scroll delivered observations"
    );
    let (painted, first) = advance(&session, initial.document);
    assert!(!painted);
    assert_eq!(first.console, ["log: entry:false:1000"]);
    session
        .send_input(DocumentInput::Scroll(ScrollInput {
            document: initial.document,
            sequence: 1,
            x: 0.0,
            y: 600.0,
        }))
        .unwrap();
    let (painted, advanced, next, queued) = next_report(&session, initial.document);
    assert!(
        !painted && !advanced,
        "scroll should schedule, not run, the observer task"
    );
    assert_eq!(
        next,
        Some(0),
        "quiet scroll must wake the browser-owned clock"
    );
    assert!(!queued.render_requested);
    assert!(queued.console.is_empty());
    let (painted, visible) = advance(&session, initial.document);
    assert!(!painted && !visible.render_requested);
    assert_eq!(visible.console, ["log: entry:true:400"]);
    assert!(
        advance(&session, initial.document).1.console.is_empty(),
        "duplicate unchanged entry"
    );
    session.shutdown().unwrap();
}

#[test]
fn native_scroll_observer_task_uses_geometry_after_nested_promise_mutations() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        120,
        r#"<!doctype html>
        <style>body{margin:0} #target{position:absolute;top:1000px;width:100px;height:100px}</style>
        <div id=target></div><script>
            const target = document.getElementById('target');
            let phase = 'initial';
            new IntersectionObserver(entries => {
                for (const e of entries) console.log('entry:' + e.isIntersecting + ':' +
                    e.boundingClientRect.top + ':' + phase);
            }).observe(target);
            document.addEventListener('scroll', event => {
                event.stopPropagation(); event.stopImmediatePropagation();
                Promise.resolve().then(() => Promise.resolve().then(() => {
                    target.style.top = '800px'; phase = 'microtasks';
                }));
            });
        </script>"#,
    );
    assert_eq!(
        advance(&session, initial.document).1.console,
        ["log: entry:false:1000:initial"]
    );
    session
        .send_input(DocumentInput::Scroll(ScrollInput {
            document: initial.document,
            sequence: 1,
            x: 0.0,
            y: 600.0,
        }))
        .unwrap();
    let (painted, advanced, next, input) = next_report(&session, initial.document);
    assert!(
        painted && !advanced,
        "Promise geometry update should render before observer delivery"
    );
    assert_eq!(next, Some(0));
    assert!(
        input.console.is_empty(),
        "observer ran inside the input task"
    );
    assert_eq!(
        advance(&session, initial.document).1.console,
        ["log: entry:true:200:microtasks"]
    );
    assert!(
        advance(&session, initial.document).1.console.is_empty(),
        "duplicate unchanged entry"
    );
    session.shutdown().unwrap();
}
