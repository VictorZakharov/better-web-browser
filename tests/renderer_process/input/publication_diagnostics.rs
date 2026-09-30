//! Input publication reasons stay opt-in; phase timing remains available without selectors.

use super::*;

#[test]
fn quiet_scroll_with_fixed_content_does_not_publish_a_full_page_for_diagnostics() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        194,
        "<!doctype html><style>body{margin:0}main{height:2500px}aside{position:fixed;top:20px}</style><main>quiet</main><aside>fixed</aside><script>globalThis.ready=true;</script>",
        vec!["html".into()],
    );
    acknowledge(&session, &initial);
    session
        .send_input(DocumentInput::Scroll(ScrollInput {
            document: initial.document,
            sequence: 1,
            x: 0.0,
            y: 20.0,
        }))
        .unwrap();
    for _ in 0..20 {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::RuntimeUpdate(update) => {
                assert_eq!(update.document, initial.document);
                assert!(!update.clock_advanced);
                assert!(!update.runtime.render_requested);
                assert!(update.runtime.errors.is_empty());
                assert!(
                    !update
                        .runtime
                        .diagnostics
                        .iter()
                        .any(|line| line.starts_with("input publication:"))
                );
                assert_eq!(update.load.text_measure_count, 0);
                return;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("quiet scroll must not become a full publication: {event:?}"),
        }
    }
    panic!("no quiet scroll response");
}

#[test]
fn zero_offset_sticky_scroll_reports_its_full_publication_reason_only_when_opted_in() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for enabled in [false, true] {
        let session = RendererSession::launch(options()).expect("hidden renderer");
        let initial = load_html_document_with_selectors(
            &session,
            if enabled { 196 } else { 195 },
            "<!doctype html><style>body{margin:0}main{padding-top:1200px;height:2500px}aside{position:sticky;top:24px;height:50px;background:red}</style><main><aside>sticky</aside></main>",
            if enabled {
                vec!["html".into()]
            } else {
                Vec::new()
            },
        );
        acknowledge(&session, &initial);
        session
            .send_input(DocumentInput::Scroll(ScrollInput {
                document: initial.document,
                sequence: 1,
                x: 0.0,
                y: 20.0,
            }))
            .unwrap();
        let updated =
            wait_for_presentation(&session, initial.document, "sticky feedback", |_| true);
        assert!(!updated.runtime.render_requested);
        assert_eq!(updated.load.text_measure_count, 0);
        let reason = updated
            .runtime
            .diagnostics
            .iter()
            .find(|line| line.starts_with("input publication:"));
        assert_eq!(reason.is_some(), enabled);
        if let Some(reason) = reason {
            assert!(
                reason.contains("kind=scroll-feedback, sequence=1"),
                "{reason}"
            );
            assert!(
                reason.contains(
                    "render_requested=false, forced_accessibility=true, rebuilt_layout=false"
                ),
                "{reason}"
            );
            assert!(reason.contains("sticky_layers=1 (displaced=0)"), "{reason}");
        }
    }
}

#[test]
fn scoped_effect_hover_preserves_immediate_style_stats_and_timing_without_diagnostics() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let html = format!(
        "<!doctype html><style>body{{margin:0}}#scope{{position:relative;width:200px;height:100px}}.target{{position:absolute;top:0;width:80px;height:50px}}#left{{left:0}}#right{{left:100px}}.target:hover{{background:red;opacity:.9}}</style><div id=scope><div id=left class=target>left</div><div id=right class=target>right</div></div><section>{}</section><script>document.title='hover-ready';</script>",
        "<span>unrelated</span>".repeat(1000),
    );
    let initial = load_html_document(&session, 197, &html);
    assert_eq!(
        initial.title, "hover-ready",
        "scripted hover route is ready"
    );
    acknowledge(&session, &initial);
    for (sequence, x) in [(1, 20.0), (2, 120.0)] {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence,
                phase: PointerPhase::Move,
                button: PointerButton::None,
                buttons: 0,
                x,
                y: 20.0,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .unwrap();
        // Opacity is geometry-equivalent but changes compositing, so this input
        // retains the immediate full-publication branch and its authoritative stats.
        let mut updated = None;
        for _ in 0..20 {
            match session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::Presentation(value) => {
                    assert_eq!(value.document, initial.document);
                    updated = Some(*value);
                    break;
                }
                RendererEvent::RuntimeUpdate(value) => {
                    assert!(
                        value.runtime.errors.is_empty(),
                        "{:?}",
                        value.runtime.errors
                    );
                }
                RendererEvent::Diagnostic { .. } | RendererEvent::PointerCursor(_) => {}
                event => panic!("unexpected immediate effect hover: {event:?}"),
            }
        }
        let updated = updated.expect("missing immediate effect hover publication");
        assert!(updated.runtime.errors.is_empty());
        assert!(updated.layout.items.iter().any(|item| matches!(item,
            DisplayItem::BeginOpacity { opacity, .. } if *opacity == 0.9)));
        assert!(
            !updated
                .runtime
                .diagnostics
                .iter()
                .any(|line| line.starts_with("input publication:"))
        );
        assert!(
            updated.load.style_micros > 0,
            "actual style gate is reported even without selectors"
        );
        if sequence == 2 {
            assert!(!updated.style.full_rebuild);
            assert!(!updated.style.layout_changed);
            assert_eq!(updated.style.changed_styles, 2);
            assert!(updated.style.total_styles > 1000);
            assert!(
                updated.style.recomputed_styles <= 8,
                "only sibling hover scope recomputed: {:?}",
                updated.style
            );
        }
        acknowledge(&session, &updated);
    }
}
