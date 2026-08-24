use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentNodeId, TestCommand, TextInput,
};
use std::time::{Duration, Instant};

#[test]
fn saturated_command_queue_recovers_and_delivers_final_input() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.unresponsive_timeout = Duration::from_secs(5);
    let mut session = RendererSession::launch(launch).expect("launch pressured renderer");
    let mut sibling = RendererSession::launch(options()).expect("launch sibling renderer");
    let initial = load_html_document(
        &session,
        150,
        r#"<!doctype html><input id="field"><p id="output">initial</p>
        <script>
            document.querySelector('#field').addEventListener('input', event => {
                document.querySelector('#output').textContent = event.target.value;
            });
        </script>"#,
    );
    let target = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) => DocumentNodeId::new(control.node_id.to_wire()).ok(),
            _ => None,
        })
        .expect("input control target");

    session
        .send_test_command(TestCommand::DelayCommandRead { millis: 1_200 })
        .expect("start finite renderer command-read stall");
    let mut saturated = false;
    for _ in 0..256 {
        match session.send_test_command(TestCommand::Padding { bytes: 60 * 1024 }) {
            Ok(()) => {}
            Err(error) if error.contains("command queue is full") => {
                saturated = true;
                break;
            }
            Err(error) => panic!("unexpected command enqueue failure: {error}"),
        }
    }
    assert!(
        saturated,
        "finite child stall did not apply bounded command backpressure"
    );

    let expected_marker = "retained-final";
    let final_input = DocumentInput::Text(TextInput {
        document: initial.document,
        sequence: 1,
        target,
        value: expected_marker.into(),
        selection_start: expected_marker.len() as u32,
        selection_end: expected_marker.len() as u32,
    });
    let mut retained = session
        .try_send_input_retained(final_input)
        .expect("renderer command channel remains connected");

    let deadline = Instant::now() + Duration::from_secs(4);
    while let Some(input) = retained {
        match session
            .try_send_input_retained(input)
            .expect("renderer command channel remains connected while pressure clears")
        {
            None => retained = None,
            Some(input) if Instant::now() < deadline => {
                retained = Some(input);
                std::thread::sleep(Duration::from_millis(10));
            }
            Some(_) => panic!("renderer command pressure did not clear"),
        }
    }

    wait_for_text(&session, initial.document, expected_marker);
    session
        .ping(Duration::from_secs(1))
        .expect("pressured renderer remains responsive");
    sibling
        .ping(Duration::from_secs(1))
        .expect("sibling renderer remains responsive");
    assert_eq!(session.snapshot().state, RendererState::Running);
    assert_eq!(sibling.snapshot().state, RendererState::Running);

    session.shutdown().expect("shutdown pressured renderer");
    sibling.shutdown().expect("shutdown sibling renderer");
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
            Ok(RendererEvent::Diagnostic { .. } | RendererEvent::TimeAdvanced { .. }) | Err(_) => {}
            Ok(event) => panic!("unexpected backpressure event: {event:?}"),
        }
    }
    panic!("retained final input was not presented");
}
