use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, InputModifiers, PointerButton, PointerInput, PointerPhase,
    PresentationAcknowledgement,
};
use std::time::Duration;

#[test]
fn hit_tested_pointer_drag_delivers_default_link_uri_to_accepted_drop() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document(
        &session,
        142,
        r#"<!doctype html><style>
            html, body { margin: 0; }
            #source { position: absolute; left: 20px; top: 20px; width: 120px; height: 80px;
                background: red; }
            #target { position: absolute; left: 250px; top: 20px; width: 120px; height: 80px;
                background: blue; }
        </style><a id="source" href="/destination">drag</a><div id="target">drop</div>
        <p id="status">waiting</p><script>
            const source = document.getElementById('source');
            const target = document.getElementById('target');
            const events = [];
            source.onpointercancel = event => events.push('cancel:' + event.isTrusted);
            source.ondragstart = event => {
                events.push('start:' + event.dataTransfer.getData('text/uri-list'));
                event.dataTransfer.effectAllowed = 'link';
            };
            target.ondragenter = event => event.preventDefault();
            target.ondragover = event => {
                events.push('over:' + event.dataTransfer.dropEffect);
                event.preventDefault();
            };
            target.ondrop = event => {
                events.push('drop:' + event.dataTransfer.getData('text/uri-list'));
                event.preventDefault();
            };
            source.ondragend = event => {
                events.push('end:' + event.dataTransfer.dropEffect);
                document.getElementById('status').textContent = events.join('|');
            };
        </script>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    for (sequence, phase, buttons, x) in [
        (1, PointerPhase::Down, PointerButton::Primary.mask(), 30.0),
        (2, PointerPhase::Move, PointerButton::Primary.mask(), 270.0),
        (3, PointerPhase::Up, 0, 270.0),
    ] {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence,
                phase,
                button: PointerButton::Primary,
                buttons,
                x,
                y: 30.0,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .unwrap();
    }
    let mut text = String::new();
    for _ in 0..12 {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation)
                if presentation.document == initial.document =>
            {
                text = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if text.contains("end:link") {
                    break;
                }
            }
            RendererEvent::Diagnostic { .. }
            | RendererEvent::RuntimeUpdate(_)
            | RendererEvent::PointerCursor(_) => {}
            event => panic!("unexpected renderer drag event: {event:?}"),
        }
    }
    assert!(text.contains("end:link"), "{text}");
    assert!(text.contains("cancel:true"), "{text}");
    assert!(
        text.contains("start:https://example.test/destination"),
        "{text}"
    );
    assert!(text.contains("over:link"), "{text}");
    assert!(
        text.contains("drop:https://example.test/destination"),
        "{text}"
    );
}
