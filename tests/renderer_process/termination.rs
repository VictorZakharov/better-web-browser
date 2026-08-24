use super::support::*;
use better_web_browser::renderer_process::{RendererExitReason, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentNodeId, TestCommand, TextInput,
};
use std::time::Duration;

#[test]
fn browser_owned_termination_stops_the_real_renderer_and_allows_replacement() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch renderer");
    let original = session.snapshot();
    load_inline_document(&session, 120);
    session.terminate().expect("request renderer termination");
    let exit = session
        .wait_for_exit(Duration::from_secs(3))
        .expect("renderer termination exit");
    assert_eq!(exit.reason, RendererExitReason::Terminated);
    assert!(
        exit.crash_surface()
            .is_some_and(|surface| surface.can_reload)
    );
    drop(session);

    let mut replacement = RendererSession::launch(options()).expect("launch replacement renderer");
    assert_ne!(replacement.snapshot().process_id, original.process_id);
    load_inline_document(&replacement, 121);
    replacement
        .shutdown()
        .expect("shutdown replacement renderer");
}

#[test]
fn browser_owned_termination_bypasses_a_saturated_page_command_queue() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.unresponsive_timeout = Duration::from_secs(10);
    let session = RendererSession::launch(launch).expect("launch renderer");
    let presentation = load_inline_document(&session, 122);
    session
        .send_test_command(TestCommand::Hang)
        .expect("hang renderer child");

    let target = DocumentNodeId::new((1_u128 << 64) | 1).unwrap();
    let value = "x".repeat(60 * 1024);
    let mut saturated = false;
    for sequence in 1..=256 {
        match session.send_input(DocumentInput::Text(TextInput {
            document: presentation.document,
            sequence,
            target,
            value: value.clone(),
            selection_start: value.len() as u32,
            selection_end: value.len() as u32,
        })) {
            Ok(()) => {}
            Err(error) if error.contains("command queue is full") => {
                saturated = true;
                break;
            }
            Err(error) => panic!("unexpected input enqueue failure: {error}"),
        }
    }
    assert!(
        saturated,
        "hung renderer did not apply command backpressure"
    );

    session
        .terminate()
        .expect("direct browser-owned Job termination");
    let exit = session
        .wait_for_exit(Duration::from_secs(3))
        .expect("renderer termination exit");
    assert_eq!(exit.reason, RendererExitReason::Terminated);
    assert!(
        exit.crash_surface()
            .is_some_and(|surface| surface.can_reload)
    );
}
