//! The isolated process must wake an idle consumer without renderer-event polling.

use super::*;
use std::sync::mpsc;

fn until_notified(
    session: &RendererSession,
    wakes: &mpsc::Receiver<()>,
    predicate: impl Fn(&RendererEvent) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        wakes
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|error| {
                panic!(
                    "renderer must notify an idle consumer: {error}; {:?}",
                    session.snapshot()
                )
            });
        let mut matched = false;
        loop {
            for _ in 0..32 {
                let Ok(Some(event)) = session.try_event() else {
                    break;
                };
                matched |= predicate(&event);
                assert!(
                    !matches!(event, RendererEvent::Exited(_)) || matched,
                    "unexpected renderer exit: {event:?}"
                );
            }
            if !session.finish_event_drain() {
                break;
            }
            assert!(Instant::now() < deadline, "renderer never yields");
            std::thread::yield_now();
        }
        if matched {
            return;
        }
    }
}

#[test]
fn renderer_wakes_for_documents_after_idle_and_for_process_failure() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let session = RendererSession::launch(options()).expect("launch hidden renderer");
    let (wake, wakes) = mpsc::channel();
    session.set_event_notifier(move || {
        let _ = wake.send(());
    });
    for value in [701, 702] {
        let document = better_web_browser::renderer_protocol::DocumentId::new(value).unwrap();
        let body =
            format!("<!doctype html><title>document {value}</title><p>ready</p>").into_bytes();
        session
            .load_document(
                document_start(document, body.len()),
                empty_document_state(),
                body,
            )
            .unwrap();
        until_notified(
            &session,
            &wakes,
            |event| matches!(event, RendererEvent::Presentation(p) if p.document == document),
        );
        session.cancel_document(document).unwrap();
    }
    session
        .send_test_command(TestCommand::InternalError)
        .unwrap();
    until_notified(&session, &wakes, |event| {
        matches!(event, RendererEvent::Exited(_))
    });
    assert_eq!(session.snapshot().state, RendererState::Exited);
}
