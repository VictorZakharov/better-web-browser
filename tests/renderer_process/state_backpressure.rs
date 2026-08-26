use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::TestCommand;
use better_web_browser::storage::{StorageAreaKind, StorageAreaSnapshot, StorageEntry};
use std::time::{Duration, Instant};

#[test]
fn web_storage_correction_survives_saturated_command_queue() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.unresponsive_timeout = Duration::from_secs(5);
    let mut session = RendererSession::launch(launch).expect("launch storage renderer");
    let initial = load_html_document(
        &session,
        157,
        r#"<!doctype html><p>waiting</p><script>
            setTimeout(() => {
                document.querySelector('p').textContent = localStorage.getItem('marker');
            }, 2000);
        </script>"#,
    );
    saturate_command_queue(&session);

    for version in 2..=257 {
        session
            .update_storage_snapshot(
                initial.document,
                StorageAreaKind::Local,
                StorageAreaSnapshot {
                    version,
                    entries: vec![StorageEntry {
                        key: "marker".into(),
                        value: format!("accepted-{version}"),
                    }],
                },
            )
            .expect("authoritative storage state does not compete with ordinary commands");
    }
    let pressure = session.snapshot();
    assert_eq!(pressure.submitted_state_updates, 256);
    // One correction may already be crossing the pipe while the newest value remains coalesced.
    assert!(pressure.pending_state_updates <= 2);

    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        match session.ping(Duration::from_secs(3)) {
            Ok(()) => break,
            Err(error) if error.contains("command queue is full") && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("storage correction pressure did not clear: {error}"),
        }
    }
    session
        .advance_time(initial.document, Duration::from_secs(2), 1)
        .expect("queue the timer after the storage correction");
    wait_for_text(&session, initial.document, "accepted-257");
    session
        .ping(Duration::from_secs(3))
        .expect("storage correction pressure remains recoverable");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().expect("shutdown storage renderer");
}

fn saturate_command_queue(session: &RendererSession) {
    session
        .send_test_command(TestCommand::DelayCommandRead { millis: 1_200 })
        .expect("start finite renderer command-read stall");
    for _ in 0..256 {
        match session.send_test_command(TestCommand::Padding { bytes: 60 * 1024 }) {
            Ok(()) => {}
            Err(error) if error.contains("command queue is full") => return,
            Err(error) => panic!("unexpected command enqueue failure: {error}"),
        }
    }
    panic!("finite child stall did not apply bounded command backpressure");
}

fn wait_for_text(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    expected: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match session.wait_for_event(Duration::from_millis(500)) {
            Ok(RendererEvent::Presentation(presentation)) if presentation.document == document => {
                let text = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                if text.contains(expected) {
                    return;
                }
            }
            Ok(RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_)) | Err(_) => {}
            Ok(event) => panic!("unexpected storage backpressure event: {event:?}"),
        }
    }
    panic!("final authoritative Web Storage state was not presented");
}
