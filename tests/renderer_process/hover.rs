use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, InputModifiers, PointerButton, PointerInput, PointerPhase,
    PresentationAcknowledgement,
};
use std::time::{Duration, Instant};

#[test]
fn hover_controls_repaint_on_native_entry_and_exit_with_and_without_script() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for script in ["", "<script>document.body.onmouseenter=()=>{};</script>"] {
        let session = RendererSession::launch(options()).expect("hidden renderer");
        let initial = load_html_document(
            &session,
            109,
            &format!(
                r#"<!doctype html>
          <style>body {{margin:0}} #player {{width:200px;height:100px;background:red}}
          #player:hover {{background:rgb(0,128,0)}}</style><div id=player></div>{script}"#
            ),
        );
        session
            .acknowledge_presentation(PresentationAcknowledgement {
                document: initial.document,
                revision: initial.revision,
                presented: true,
                controls_applied: true,
            })
            .unwrap();
        for (sequence, phase, expected) in [
            (1, PointerPhase::Move, (0, 128, 0)),
            (2, PointerPhase::Leave, (255, 0, 0)),
        ] {
            session
                .send_input(DocumentInput::Pointer(PointerInput {
                    document: initial.document,
                    sequence,
                    phase,
                    button: PointerButton::None,
                    x: 20.0,
                    y: 20.0,
                    modifiers: InputModifiers::default(),
                    target: None,
                }))
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                assert!(Instant::now() < deadline, "no hover repaint for {phase:?}");
                if let RendererEvent::Presentation(presentation) =
                    session.wait_for_event(Duration::from_secs(2)).unwrap()
                {
                    session
                        .acknowledge_presentation(PresentationAcknowledgement {
                            document: presentation.document,
                            revision: presentation.revision,
                            presented: true,
                            controls_applied: true,
                        })
                        .unwrap();
                    if presentation.layout.items.iter().any(|item| {
                        matches!(item,
                        DisplayItem::SolidRect {rect,color,..} if rect.width==200.0 &&
                        (color.red,color.green,color.blue)==expected)
                    }) {
                        break;
                    }
                }
            }
        }
    }
}
