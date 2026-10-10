//! Silent pointer paths must complete browser-side motion backpressure.
use super::*;

fn motion(
    document: better_web_browser::renderer_protocol::DocumentId,
    sequence: u64,
) -> DocumentInput {
    DocumentInput::Pointer(PointerInput {
        document,
        sequence,
        phase: PointerPhase::Move,
        button: PointerButton::None,
        buttons: 0,
        x: 20.0,
        y: 20.0,
        modifiers: InputModifiers::default(),
        target: None,
    })
}

#[test]
fn pointer_completion_survives_rejected_sequence_and_stale_document_input() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let unloaded = better_web_browser::renderer_protocol::DocumentId::new(40).unwrap();
    let initial = load_html_document(&session, 41, "<div>ready</div>");
    session.send_input(motion(initial.document, 3)).unwrap();
    assert_eq!(
        wait_for_cursor(&session, initial.document, 3),
        PointerCursor::Default
    );
    session.send_input(motion(initial.document, 2)).unwrap();
    assert_eq!(
        wait_for_cursor(&session, initial.document, 2),
        PointerCursor::Default
    );
    session.send_input(motion(unloaded, 4)).unwrap();
    session.send_input(motion(initial.document, 5)).unwrap();
    assert_eq!(
        wait_for_cursor(&session, initial.document, 5),
        PointerCursor::Default
    );
    session.shutdown().unwrap();
}
