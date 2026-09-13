use super::*;

#[test]
fn pointer_designation_without_style_changes_does_not_rebuild_layout_or_republish_it() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        166,
        "<!doctype html><style>body{margin:0}div{height:100px}</style><div>unchanged</div><div>also unchanged</div>",
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    for (sequence, y) in [(1, 30.0), (2, 130.0), (3, 30.0)] {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence,
                phase: PointerPhase::Move,
                button: PointerButton::None,
                buttons: 0,
                x: 30.0,
                y,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .unwrap();
        let mut completed = false;
        for _ in 0..20 {
            match session.wait_for_event(Duration::from_secs(5)).unwrap() {
                RendererEvent::RuntimeUpdate(update) => {
                    assert!(!update.runtime.render_requested);
                    assert_eq!(update.load.text_measure_count, 0);
                    completed = true;
                    break;
                }
                RendererEvent::Diagnostic { .. } | RendererEvent::PointerCursor(_) => {}
                event => panic!("no-op hover must not send a complete page: {event:?}"),
            }
        }
        assert!(completed);
    }
}

#[test]
fn process_boundary_preserves_sticky_constraints_for_a_newer_native_scroll_offset() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        167,
        "<!doctype html><style>body{margin:0}main{padding-top:100px;height:2500px}aside{position:sticky;top:24px;height:50px;background:red}</style><main><aside>sticky</aside></main>",
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
        .send_input(DocumentInput::Scroll(ScrollInput {
            document: initial.document,
            sequence: 1,
            x: 0.0,
            y: 900.0,
        }))
        .unwrap();
    let mut presentation = None;
    for _ in 0..20 {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(value) => {
                presentation = Some(value);
                break;
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected scroll result: {event:?}"),
        }
    }
    let presentation = presentation.expect("sticky scroll presentation");
    assert_eq!(presentation.load.text_measure_count, 0);
    let mut retained = presentation.layout.into_layout();
    assert!(!retained.sticky_layers.is_empty());
    for (scroll, expected) in [(0.0, 100.0), (400.0, 424.0), (1500.0, 1524.0), (0.0, 100.0)] {
        retained.update_scroll_position(0.0, scroll);
        let y = retained
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::SolidRect { rect, color, .. }
                    if color.red == 255 && color.green == 0 =>
                {
                    Some(rect.y)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(y, expected);
    }
}
