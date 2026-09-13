use super::*;
use better_web_browser::renderer_protocol::WheelInput;

fn wheel(
    document: better_web_browser::renderer_protocol::DocumentId,
    sequence: u64,
    delta: f32,
) -> DocumentInput {
    DocumentInput::Wheel(WheelInput {
        document,
        sequence,
        x: 50.0,
        y: 50.0,
        delta_x: 0.0,
        delta_y: delta,
        viewport_y: 0.0,
        modifiers: InputModifiers::default(),
    })
}

#[test]
fn wheel_scrolls_inner_pane_and_clipped_content_receives_hits_at_its_visual_position() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        160,
        r#"<!doctype html><style>
        body{margin:0} #pane{width:200px;height:100px;overflow:auto}
        #first{height:100px;background:red}#second{height:100px;background:blue}#tail{height:300px}
        </style><div id=pane><div id=first></div><div id=second></div><div id=tail></div></div>
        <p id=status>ready</p><script>
        const pane=document.getElementById('pane'), status=document.getElementById('status');
        pane.addEventListener('scroll',()=>status.textContent='scrolled:'+pane.scrollTop+':viewport:'+scrollY);
        pane.addEventListener('click',e=>status.textContent='clicked:'+e.target.id);
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
    session
        .send_input(wheel(initial.document, 1, 75.0))
        .unwrap();
    let mut changed = None;
    for _ in 0..30 {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                if presentation_text(&presentation).contains("scrolled:75:viewport:0") {
                    changed = Some(presentation);
                    break;
                }
                pump_ready_task(&session, initial.document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                pump_ready_task(&session, initial.document, update.next_timer_micros)
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected scroll event: {event:?}"),
        }
    }
    let changed = changed.expect("element scroll event crossed process boundary");
    assert!(
        changed
            .layout
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::SolidRect {rect,color,..}
        if color.blue==255 && color.red==0 && rect.y==25.0))
    );
    session
        .send_input(DocumentInput::Pointer(PointerInput {
            document: initial.document,
            sequence: 2,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            buttons: 0,
            x: 50.0,
            y: 50.0,
            modifiers: InputModifiers::default(),
            target: None,
        }))
        .unwrap();
    let clicked = wait_for_text(&session, initial.document, "clicked:second");
    assert!(presentation_text(&clicked).contains("clicked:second"));
    for (sequence, phase, y, buttons) in [
        (3, PointerPhase::Down, 20.0, 1),
        (4, PointerPhase::Move, 58.0, 1),
        (5, PointerPhase::Up, 58.0, 0),
    ] {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence,
                phase,
                button: if phase == PointerPhase::Move {
                    PointerButton::None
                } else {
                    PointerButton::Primary
                },
                buttons,
                x: 194.0,
                y,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .unwrap();
    }
    let mut dragged = false;
    for _ in 0..30 {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                if presentation_text(&presentation).contains("scrolled:275:viewport:0") {
                    dragged = true;
                    break;
                }
                pump_ready_task(&session, initial.document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                pump_ready_task(&session, initial.document, update.next_timer_micros)
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected drag event:{event:?}"),
        }
    }
    assert!(dragged, "dragging the scrollbar thumb must scroll the pane");
}

#[test]
fn canceling_a_native_wheel_prevents_the_scroll_default_action() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        161,
        r#"<!doctype html><style>body{margin:0}
        #pane{width:200px;height:100px;overflow:auto}#content{height:500px}</style>
        <div id=pane><div id=content></div></div><p id=status>ready</p><script>
        document.getElementById('pane').addEventListener('wheel',e=>{
            e.preventDefault(); document.getElementById('status').textContent='cancelled:'+document.getElementById('pane').scrollTop+':'+e.isTrusted;
        },{passive:false});</script>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    session
        .send_input(wheel(initial.document, 1, 75.0))
        .unwrap();
    let result = wait_for_text(&session, initial.document, "cancelled:0:true");
    assert!(presentation_text(&result).contains("cancelled:0:true"));
}

#[test]
fn viewport_scroll_repositions_sticky_paint_without_page_javascript() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        162,
        r#"<!doctype html><style>
        body{margin:0}main{height:1200px}aside{position:sticky;top:24px;width:200px;height:60px;background:red}
        </style><main><aside>pinned</aside></main><div style="height:1000px"></div>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    for (sequence, y) in [(1, 150.0), (2, 300.0), (3, 0.0)] {
        session
            .send_input(DocumentInput::Scroll(ScrollInput {
                document: initial.document,
                sequence,
                x: 0.0,
                y,
            }))
            .unwrap();
        let next = wait_for_text(&session, initial.document, "pinned");
        assert!(next.layout.items.iter().any(|item| matches!(item,
            better_web_browser::engine::DisplayItem::SolidRect {rect,color,..}
            if color.red==255 && color.blue==0 && rect.y==y+24.0)));
        session
            .acknowledge_presentation(PresentationAcknowledgement {
                document: next.document,
                revision: next.revision,
                presented: true,
                controls_applied: true,
            })
            .unwrap();
    }
}
