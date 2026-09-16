use super::support::*;
use better_web_browser::limits::{MAX_RUNTIME_REPORT_ENTRIES, MAX_RUNTIME_REPORT_TEXT_BYTES};
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::PresentationAcknowledgement;
use std::time::Duration;

#[test]
fn noisy_author_console_is_reported_without_stopping_the_isolated_renderer() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let page = load_html_document(
        &session,
        1,
        r#"<!doctype html><script>
        for(let i=0;i<700;i++)console.log('message '+i);
        console.log('\u{1f600}'.repeat(70000));
        document.title='still running';
        setTimeout(() => { document.title='next task'; }, 10000);
        </script><p>Page survives diagnostic output pressure</p>"#,
    );
    assert_eq!(page.title, "still running");
    assert!(page.runtime.console.len() <= MAX_RUNTIME_REPORT_ENTRIES);
    assert!(
        page.runtime.console.iter().map(String::len).sum::<usize>()
            <= MAX_RUNTIME_REPORT_TEXT_BYTES
    );
    assert!(
        page.runtime
            .console
            .last()
            .unwrap()
            .contains("entries omitted")
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: page.document,
            revision: page.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    session
        .advance_time(page.document, Duration::from_secs(11), 1)
        .unwrap();
    let next = loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(next) => break next,
            RendererEvent::RuntimeUpdate(update) => {
                pump_ready_task(&session, page.document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event after noisy console: {event:?}"),
        }
    };
    assert_eq!(next.title, "next task");
    assert!(next.runtime.errors.is_empty());
    session.shutdown().unwrap();
}
