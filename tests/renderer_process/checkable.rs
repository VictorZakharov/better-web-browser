use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, InputModifiers, PointerButton, PointerInput, PointerPhase,
    PresentationAcknowledgement,
};
use std::time::{Duration, Instant};

#[test]
fn radio_label_click_repaints_live_selection_with_and_without_javascript() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for script in ["", "<script>document.body.dataset.ready='yes'</script>"] {
        let session = RendererSession::launch(options()).expect("hidden renderer");
        let initial = load_html_document(
            &session,
            111,
            &format!(
                r#"<!doctype html>
        <style>body{{margin:0}}label{{display:block;width:200px;height:40px}}
        input{{position:absolute;opacity:0;width:20px;height:20px}}
        span{{display:block;width:100px;height:30px;background:red}}
        input:checked+span{{background:rgb(0,128,0)}}</style>
        <label><input type=radio name=group checked><span>First</span></label>
        <label><input type=radio name=group><span>Second</span></label>{script}"#
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
        for (sequence, phase) in [(1, PointerPhase::Down), (2, PointerPhase::Up)] {
            session
                .send_input(DocumentInput::Pointer(PointerInput {
                    document: initial.document,
                    sequence,
                    phase,
                    button: PointerButton::Primary,
                    buttons: if phase == PointerPhase::Down { 1 } else { 0 },
                    x: 60.0,
                    y: 50.0,
                    modifiers: InputModifiers::default(),
                    target: None,
                }))
                .unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                Instant::now() < deadline,
                "radio did not repaint, script={script:?}"
            );
            if let RendererEvent::Presentation(presentation) =
                session.wait_for_event(Duration::from_secs(1)).unwrap()
            {
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document: presentation.document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                let selected = |top| {
                    presentation.layout.items.iter().any(|item| matches!(item,
                    DisplayItem::SolidRect {rect,color,..} if rect.width==100.0 && rect.y==top && color.green==128))
                };
                if selected(40.0) && !selected(0.0) {
                    break;
                }
            }
        }
    }
}
