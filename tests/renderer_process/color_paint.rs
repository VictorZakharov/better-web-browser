//! A visual rendering opportunity must not defer author events or wheel decisions.
use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentInput, InputModifiers, PointerButton, PointerInput, PointerPhase,
    RendererPresentation, WheelAcknowledgement, WheelDecision, WheelInput,
};
use std::time::Duration;

fn move_pointer(session: &RendererSession, document: DocumentId, sequence: u64, x: f32) {
    session
        .send_input(DocumentInput::Pointer(PointerInput {
            document,
            sequence,
            phase: PointerPhase::Move,
            button: PointerButton::None,
            buttons: 0,
            x,
            y: 50.0,
            modifiers: InputModifiers::default(),
            target: None,
        }))
        .unwrap();
}

fn wheel(session: &RendererSession, document: DocumentId, sequence: u64, delta_y: f32) {
    session
        .send_input(DocumentInput::Wheel(WheelInput {
            document,
            sequence,
            x: 20.0,
            y: 50.0,
            viewport_y: 0.0,
            delta_x: 0.0,
            delta_y,
            modifiers: InputModifiers::default(),
            target: None,
        }))
        .unwrap();
}

fn next(session: &RendererSession) -> RendererEvent {
    session.wait_for_event(Duration::from_secs(3)).unwrap()
}

fn paint(session: &RendererSession) -> RendererPresentation {
    for _ in 0..40 {
        match next(session) {
            RendererEvent::Presentation(value) => {
                assert!(
                    value.runtime.errors.is_empty(),
                    "{:?}",
                    value.runtime.errors
                );
                return *value;
            }
            RendererEvent::RuntimeUpdate(value) => {
                assert!(
                    value.runtime.errors.is_empty(),
                    "{:?}",
                    value.runtime.errors
                );
                pump_ready_task(session, value.document, value.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::PointerCursor(_) => {}
            event => panic!("unexpected paint event: {event:?}"),
        }
    }
    panic!("missing presentation");
}

fn await_deferred_paint(session: &RendererSession, document: DocumentId) -> RendererPresentation {
    // Production admission uses wall time, not the author timer's logical clock. The hidden
    // session has no native HWND scheduler, so service one expired rendering opportunity.
    std::thread::sleep(Duration::from_millis(25));
    session.advance_time(document, Duration::ZERO, 64).unwrap();
    paint(session)
}

fn has_pad(presentation: &RendererPresentation, width: f32, green: bool) -> bool {
    presentation.layout.items.iter().any(|item| {
        matches!(item,
        DisplayItem::SolidRect { rect, color, .. }
        if rect.width == width && rect.height == 100.0
        && (color.red, color.green, color.blue) == if green { (0,128,0) } else { (255,0,0) })
    })
}

#[test]
fn color_hover_batches_one_paint_but_delivers_every_move_and_wheel_verdict_first() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(&session, 310, include_str!("../fixtures/color-paint.html"));
    acknowledge(&session, &initial);
    for (sequence, x) in [(1, 20.0), (2, 320.0), (3, 20.0), (4, 320.0), (5, 20.0)] {
        move_pointer(&session, initial.document, sequence, x);
    }
    wheel(&session, initial.document, 6, 126.0);
    wheel(&session, initial.document, 7, -126.0);

    let mut console = Vec::new();
    let mut verdicts: Vec<WheelAcknowledgement> = Vec::new();
    let mut advertised = None;
    for _ in 0..40 {
        match next(&session) {
            RendererEvent::RuntimeUpdate(update) => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                console.extend(update.runtime.console);
                verdicts.extend(update.runtime.wheel_acknowledgements);
                advertised = update.next_timer_micros;
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::PointerCursor(_) => {}
            RendererEvent::Presentation(_) => panic!("visual batching delayed an input verdict"),
            event => panic!("unexpected input event: {event:?}"),
        }
        if verdicts.len() == 2 {
            break;
        }
    }
    let moves = console
        .iter()
        .filter(|line| line.contains("MOVE:"))
        .collect::<Vec<_>>();
    assert_eq!(moves.len(), 5, "every author move must run: {console:?}");
    for (index, target) in ["pad", "outside", "pad", "outside", "pad"]
        .iter()
        .enumerate()
    {
        assert!(moves[index].contains(&format!("MOVE:{}:{target}:200:", index + 1)));
    }
    assert!(
        moves[4].contains("rgb(0, 128, 0)"),
        "current computed style: {moves:?}"
    );
    assert!(console.iter().any(|line| line.contains("WHEEL:5:126")));
    assert!(console.iter().any(|line| line.contains("WHEEL:5:-126")));
    assert_eq!(
        verdicts
            .iter()
            .map(|ack| (ack.sequence, ack.decision))
            .collect::<Vec<_>>(),
        [(6, WheelDecision::Viewport), (7, WheelDecision::Cancelled)]
    );
    assert_eq!(verdicts[0].viewport_delta_y, 126.0);
    assert_eq!(verdicts[1].viewport_delta_y, 0.0);
    // The native shell subtracts elapsed wall time from this logical-clock-relative
    // delay. Exact 16 ms admission and no-restart bounds are covered by policy tests.
    assert!(
        advertised.is_some(),
        "the deferred paint must advertise a wakeup"
    );

    let final_paint = await_deferred_paint(&session, initial.document);
    assert!(has_pad(&final_paint, 200.0, true));
    assert!(
        final_paint.runtime.wheel_acknowledgements.is_empty(),
        "paint must not repeat verdicts"
    );
    let text_rects = final_paint
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text {
                rect, text, color, ..
            } if !text.trim().is_empty()
                && (color.red, color.green, color.blue) == (255, 255, 255) =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let paint_items = final_paint
        .layout
        .items
        .iter()
        .filter(|item| {
            matches!(
                item,
                DisplayItem::Text { .. } | DisplayItem::SolidRect { .. }
            )
        })
        .collect::<Vec<_>>();
    assert!(
        !text_rects.is_empty(),
        "the latest hover must paint white glyphs; items={paint_items:#?}; input effects={console:#?}"
    );
    assert!(
        text_rects.iter().any(
            |text| final_paint.layout.items.iter().any(|item| matches!(item,
        DisplayItem::SolidRect { rect, color, .. }
        if (color.red, color.green, color.blue) == (255,255,255)
        && rect.x == text.x && rect.width == text.width
        && rect.height >= 1.0 && rect.height <= 3.0
        && (rect.y + rect.height - (text.y + text.height)).abs() < 0.01))
        ),
        "the latest hover must paint a thin white underline along the glyph run's bottom; \
         text rects={text_rects:#?}; text/fonts and solid rects={paint_items:#?}; \
         input effects={console:#?}"
    );
    acknowledge(&session, &final_paint);
    session
        .advance_time(initial.document, Duration::ZERO, 64)
        .unwrap();
    let mut completed = false;
    for _ in 0..40 {
        match next(&session) {
            RendererEvent::RuntimeUpdate(update) if update.clock_advanced => {
                completed = true;
                break;
            }
            RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Diagnostic { .. }
            | RendererEvent::PointerCursor(_) => {}
            RendererEvent::Presentation(_) => {
                panic!("one burst must not replay a second pending paint")
            }
            event => panic!("unexpected completed-paint event: {event:?}"),
        }
    }
    assert!(
        completed,
        "missing completed clock report after the single paint"
    );
    session.shutdown().unwrap();
}

#[test]
fn author_geometry_flush_absorbs_pending_color_and_updates_the_next_native_hit() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let html = include_str!("../fixtures/color-paint-author.html").replace(
        "/* AUTHOR */",
        r#"
        if (moves === 2) {
          pad.style.width = '260px';
          console.log('GEOMETRY:' + pad.getBoundingClientRect().width);
        }
    "#,
    );
    let initial = load_html_document(&session, 311, &html);
    acknowledge(&session, &initial);
    move_pointer(&session, initial.document, 1, 20.0);
    let mut first_move = false;
    for _ in 0..40 {
        match next(&session) {
            RendererEvent::RuntimeUpdate(update)
                if update
                    .runtime
                    .console
                    .iter()
                    .any(|line| line.contains("HIT:1:pad")) =>
            {
                first_move = true;
                break;
            }
            RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Diagnostic { .. }
            | RendererEvent::PointerCursor(_) => {}
            event => panic!("first color-only move must remain runtime-only: {event:?}"),
        }
    }
    assert!(
        first_move,
        "missing first author move before deferred paint"
    );
    move_pointer(&session, initial.document, 2, 21.0);
    let resized = paint(&session);
    assert!(
        has_pad(&resized, 260.0, true),
        "author geometry must publish immediately"
    );
    assert!(
        resized
            .runtime
            .console
            .iter()
            .any(|line| line.contains("GEOMETRY:260"))
    );
    acknowledge(&session, &resized);
    move_pointer(&session, initial.document, 3, 240.0);
    let mut correct_hit = false;
    for _ in 0..40 {
        match next(&session) {
            RendererEvent::RuntimeUpdate(update)
                if update
                    .runtime
                    .console
                    .iter()
                    .any(|line| line.contains("HIT:3:pad")) =>
            {
                correct_hit = true;
                break;
            }
            RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Diagnostic { .. }
            | RendererEvent::PointerCursor(_) => {}
            event => panic!("native hit must use the already settled 260 px box: {event:?}"),
        }
    }
    assert!(correct_hit, "missing hit in the expanded 260 px box");
    session.shutdown().unwrap();
}

#[test]
fn non_color_hover_effects_and_hit_changes_are_not_deferred() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    for (index, effect) in ["opacity:0.25", "visibility:hidden", "pointer-events:none"]
        .iter()
        .enumerate()
    {
        let mut session = RendererSession::launch(options()).expect("hidden renderer");
        let html =
            include_str!("../fixtures/color-paint-author.html").replace("/* EFFECT */", effect);
        let initial = load_html_document(&session, 312 + index as u64, &html);
        acknowledge(&session, &initial);
        move_pointer(&session, initial.document, 1, 20.0);
        let changed = paint(&session);
        assert!(
            changed
                .runtime
                .console
                .iter()
                .any(|line| line.contains("HIT:1:pad"))
        );
        if index == 0 {
            assert!(changed.layout.items.iter().any(|item| matches!(item,
                DisplayItem::BeginOpacity { opacity, .. } if *opacity == 0.25)));
        } else if index == 1 {
            assert!(
                !has_pad(&changed, 200.0, true),
                "hidden pad cannot retain old paint"
            );
        }
        acknowledge(&session, &changed);
        if index != 0 {
            move_pointer(&session, initial.document, 2, 20.0);
            let next_paint = paint(&session);
            assert!(
                next_paint
                    .runtime
                    .console
                    .iter()
                    .any(|line| line.contains("HIT:2:"))
            );
            assert!(
                next_paint
                    .runtime
                    .console
                    .iter()
                    .all(|line| !line.contains("HIT:2:pad")),
                "hidden/non-targetable pad must not own the next native input"
            );
        }
        session.shutdown().unwrap();
    }
}
