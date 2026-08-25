use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::TestCommand;
use std::time::Duration;

#[test]
fn document_clock_survives_a_saturated_page_command_queue() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.unresponsive_timeout = Duration::from_secs(5);
    let mut session = RendererSession::launch(launch).expect("launch clock renderer");
    let initial = load_inline_document(&session, 157);

    saturate_page_command_queue(&session);
    for _ in 0..32 {
        session
            .advance_time(initial.document, Duration::from_millis(50), 1)
            .expect("retain document-clock progress independently from page commands");
    }

    loop {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::TimeAdvanced { document, .. } if document == initial.document => break,
            RendererEvent::Presentation(presentation)
                if presentation.document == initial.document =>
            {
                break;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected document-clock event: {event:?}"),
        }
    }
    session
        .ping(Duration::from_secs(1))
        .expect("clock renderer remains responsive");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().expect("shutdown clock renderer");
}

fn saturate_page_command_queue(session: &RendererSession) {
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
