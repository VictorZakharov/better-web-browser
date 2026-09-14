use super::*;

#[test]
fn block_link_padding_and_nested_children_follow_fragment_without_new_document() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (index, (scripted, nested)) in [(false, false), (false, true), (true, false), (true, true)]
        .into_iter()
        .enumerate()
    {
        let session = RendererSession::launch(options()).expect("launch renderer");
        let html = format!(
            r##"<!doctype html><style>body {{ margin: 0 }}
            a {{ display: block; width: 200px; padding: 20px; background: red }}
            h2 {{ display: inline; margin: 0 }}</style><a href="#section"><span>Go</span></a>
            <div style="height:900px"></div><h2 id=section>Section</h2><div style="height:900px"></div>{}"##,
            if scripted {
                "<script>window.retainedRealm = 123;</script>"
            } else {
                ""
            }
        );
        let initial = load_html_document(&session, 850 + index as u64, &html);
        let rect = initial
            .layout
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::SolidRect { rect, .. } if rect.width == 240.0 => Some(*rect),
                _ => None,
            })
            .expect("block link background");
        for (sequence, phase) in [(1, PointerPhase::Down), (2, PointerPhase::Up)] {
            session
                .send_input(DocumentInput::Pointer(PointerInput {
                    document: initial.document,
                    sequence,
                    phase,
                    button: PointerButton::Primary,
                    buttons: if phase == PointerPhase::Down { 1 } else { 0 },
                    x: if nested {
                        rect.x + 25.0
                    } else {
                        rect.right() - 5.0
                    },
                    y: if nested {
                        rect.y + 25.0
                    } else {
                        rect.bottom() - 5.0
                    },
                    modifiers: InputModifiers::default(),
                    target: None,
                }))
                .unwrap();
        }
        let mut found = false;
        for _ in 0..16 {
            let report = match session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::Presentation(p) => p.runtime,
                RendererEvent::RuntimeUpdate(p) => p.runtime,
                RendererEvent::Diagnostic { .. } => continue,
                other => panic!("unexpected fragment navigation event: {other:?}"),
            };
            assert!(report.errors.is_empty(), "{:?}", report.errors);
            assert!(report.navigation_url.is_none());
            if !report.history_updates.is_empty() {
                assert_eq!(
                    report.history_updates[0].url,
                    format!("https://example.test/{}#section", initial.document.get())
                );
                assert!(report.viewport_scroll_y.unwrap() > 900.0);
                found = true;
                break;
            }
        }
        assert!(
            found,
            "fragment activation must publish history and scroll together"
        );
    }
}
