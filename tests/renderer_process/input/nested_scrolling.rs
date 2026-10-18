use super::*;
use better_web_browser::renderer_protocol::{WheelDecision, WheelInput};

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
        target: None,
    })
}

#[test]
fn queued_viewport_wheels_keep_relative_distance_despite_stale_viewport_coordinates() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        163,
        "<!doctype html><style>body{margin:0}main{height:3000px}</style><main>long document</main>",
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    // Both arrived while the browser was at zero; no Scroll acknowledgement between them.
    session
        .send_input(wheel(initial.document, 1, 126.0))
        .unwrap();
    session
        .send_input(wheel(initial.document, 2, 126.0))
        .unwrap();
    let mut distance = 0.0;
    for _ in 0..20 {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                assert_eq!(presentation.runtime.viewport_scroll_y, None);
                distance += presentation.runtime.viewport_wheel_delta_y;
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document: presentation.document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
            }
            RendererEvent::RuntimeUpdate(update) => {
                assert_eq!(update.runtime.viewport_scroll_y, None);
                distance += update.runtime.viewport_wheel_delta_y;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected wheel event: {event:?}"),
        }
        if distance == 252.0 {
            break;
        }
    }
    assert_eq!(
        distance, 252.0,
        "neither wheel delta may be replaced by a stale absolute offset"
    );
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
    let mut motion_completed = false;
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
            RendererEvent::PointerCursor(result) => {
                assert_eq!(result.document, initial.document);
                assert_eq!(result.sequence, 4);
                assert_eq!(result.cursor, PointerCursor::Default);
                motion_completed = true;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected drag event:{event:?}"),
        }
    }
    assert!(dragged, "dragging the scrollbar thumb must scroll the pane");
    assert!(
        motion_completed,
        "scrollbar drag must release the next queued mousemove"
    );
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

/// The browser starts viewport motion before the renderer's verdict only while no nested
/// scrollport can own a wheel. A scrollport that hit testing can never reach, such as a closed
/// `visibility: hidden` dropdown menu, must not disable that, and the renderer's own verdict
/// over its box must agree.
#[test]
fn only_hit_testable_scrollports_report_nested_wheel_ownership() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (index, (pane, extra, nested)) in [
        ("", "", true),
        ("visibility:hidden", "", false),
        ("pointer-events:none", "", false),
        ("visibility:hidden", "#pane p{visibility:visible}", true),
        ("pointer-events:none", "#pane p{pointer-events:auto}", true),
    ]
    .into_iter()
    .enumerate()
    {
        let session = RendererSession::launch(options()).expect("hidden renderer");
        let html = format!(
            "<!doctype html><style>body{{margin:0}}main{{height:3000px}}\
             #pane{{position:absolute;top:0;left:0;width:200px;height:100px;overflow:auto;{pane}}}\
             #pane p{{margin:0;height:600px}}{extra}</style>\
             <div id=pane><p>menu</p></div><main>long document</main>"
        );
        let initial = load_html_document(&session, 170 + index as u64, &html);
        assert_eq!(
            initial.runtime.wheel_has_nested_scroll,
            Some(nested),
            "{pane} {extra}"
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
            .send_input(wheel(initial.document, 1, 126.0))
            .unwrap();
        let decision = loop {
            let acknowledgements = match session.wait_for_event(Duration::from_secs(5)).unwrap() {
                RendererEvent::Presentation(presentation) => {
                    presentation.runtime.wheel_acknowledgements.clone()
                }
                RendererEvent::RuntimeUpdate(update) => update.runtime.wheel_acknowledgements,
                _ => Vec::new(),
            };
            if let Some(acknowledgement) = acknowledgements.first() {
                break acknowledgement.decision;
            }
        };
        let expected = if nested {
            WheelDecision::NestedScroll
        } else {
            WheelDecision::Viewport
        };
        assert_eq!(decision, expected, "{pane} {extra}");
    }
}
