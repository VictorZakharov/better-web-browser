//! Quiet/cancelled wheel verdicts and sequence fences survive the renderer boundary.
use super::*;
use better_web_browser::renderer_protocol::{
    DocumentId, WheelAcknowledgement, WheelDecision, WheelInput,
};

const HTML: &str = include_str!("../../fixtures/wheel-timing.html");

fn wheel(document: DocumentId, sequence: u64, x: f32, delta: f32) -> DocumentInput {
    DocumentInput::Wheel(WheelInput {
        document,
        sequence,
        x,
        y: 50.0,
        viewport_y: 0.0,
        delta_x: 0.0,
        delta_y: delta,
        modifiers: InputModifiers::default(),
        target: None,
    })
}

fn collect(
    session: &RendererSession,
    document: DocumentId,
    count: usize,
) -> Vec<WheelAcknowledgement> {
    let mut values = Vec::new();
    for _ in 0..30 {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                assert_eq!(presentation.document, document);
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                values.extend(presentation.runtime.wheel_acknowledgements.iter().copied());
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
            }
            RendererEvent::RuntimeUpdate(update) => {
                assert_eq!(update.document, document);
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                values.extend(update.runtime.wheel_acknowledgements);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected wheel event: {event:?}"),
        }
        if values.len() >= count {
            return values;
        }
    }
    panic!("missing wheel verdicts: {values:?}");
}

#[test]
fn default_action_decisions_include_quiet_cancelled_nested_and_reversed_wheels() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(&session, 290, HTML);
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    for (sequence, x, delta) in [
        (1, 300.0, 126.0),
        (2, 50.0, 75.0),
        (3, 550.0, 126.0),
        (4, 550.0, -126.0),
        (5, 550.0, 0.0),
    ] {
        session
            .send_input(wheel(initial.document, sequence, x, delta))
            .unwrap();
    }
    let values = collect(&session, initial.document, 5);
    assert_eq!(
        values
            .iter()
            .map(|value| value.sequence)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    assert_eq!(
        values
            .iter()
            .map(|value| value.decision)
            .collect::<Vec<_>>(),
        [
            WheelDecision::Cancelled,
            WheelDecision::NestedScroll,
            WheelDecision::Viewport,
            WheelDecision::Viewport,
            WheelDecision::NoMotion
        ]
    );
    assert_eq!(
        values
            .iter()
            .map(|value| value.viewport_delta_y)
            .collect::<Vec<_>>(),
        [0.0, 0.0, 126.0, -126.0, 0.0]
    );
    session.shutdown().unwrap();
}

#[test]
fn stale_document_and_duplicate_sequence_never_acknowledge_a_replacement_input() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let old = load_html_document(&session, 291, HTML);
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: old.document,
            revision: old.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    session.cancel_document(old.document).unwrap();
    let new = load_html_document(&session, 292, HTML);
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: new.document,
            revision: new.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    session
        .send_input(wheel(old.document, 99, 550.0, 126.0))
        .unwrap();
    session
        .send_input(wheel(new.document, 1, 300.0, 126.0))
        .unwrap();
    session
        .send_input(wheel(new.document, 1, 550.0, 126.0))
        .unwrap();
    session
        .send_input(wheel(new.document, 2, 550.0, 0.0))
        .unwrap();
    let values = collect(&session, new.document, 2);
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].sequence, 1);
    assert_eq!(values[0].decision, WheelDecision::Cancelled);
    assert_eq!(values[1].sequence, 2);
    assert_eq!(values[1].decision, WheelDecision::NoMotion);
    session.shutdown().unwrap();
}

#[test]
fn asynchronous_scroll_reset_keeps_the_owning_motion_snapshot_and_verdict_separate() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        293,
        include_str!("../../fixtures/wheel-timing-reset.html"),
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
        .send_input(wheel(initial.document, 1, 50.0, 75.0))
        .unwrap();
    let mut owning = None;
    for _ in 0..30 {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(value) => {
                assert!(
                    value.runtime.errors.is_empty(),
                    "{:?}",
                    value.runtime.errors
                );
                assert_eq!(value.document, initial.document);
                if !value.runtime.wheel_acknowledgements.is_empty() {
                    assert!(
                        owning.is_none(),
                        "queued scroll events must not repeat the wheel verdict"
                    );
                    assert_eq!(value.runtime.wheel_acknowledgements.len(), 1);
                    assert_eq!(value.runtime.wheel_acknowledgements[0].sequence, 1);
                    assert_eq!(
                        value.runtime.wheel_acknowledgements[0].decision,
                        WheelDecision::NestedScroll
                    );
                    assert_eq!(value.runtime.viewport_wheel_delta_y, 0.0);
                    assert_eq!(
                        value.runtime.wheel_acknowledgements[0].viewport_delta_y,
                        0.0
                    );
                    assert!(
                        value
                            .runtime
                            .console
                            .iter()
                            .all(|line| !line.contains("reset:")),
                        "scroll notification must not run inside the wheel default action"
                    );
                    assert!(value.layout.items.iter().any(|item| matches!(item,
                        DisplayItem::SolidRect { rect, color, .. } if rect.y == -75.0 && color.red == 255 && color.blue == 0)),
                        "the owning presentation must retain the initial 75 px nested motion");
                    session
                        .acknowledge_presentation(PresentationAcknowledgement {
                            document: value.document,
                            revision: value.revision,
                            presented: true,
                            controls_applied: true,
                        })
                        .unwrap();
                    pump_ready_task(&session, initial.document, value.next_timer_micros);
                    owning = Some(*value);
                    continue;
                }
                if value
                    .runtime
                    .console
                    .iter()
                    .any(|line| line.contains("offset after reset:0"))
                {
                    let nested = owning
                        .as_ref()
                        .expect("wheel motion must precede its queued notification");
                    assert!(value.revision > nested.revision);
                    assert_eq!(value.runtime.viewport_wheel_delta_y, 0.0);
                    assert!(
                        value
                            .runtime
                            .console
                            .iter()
                            .any(|line| line.contains("offset before reset:75"))
                    );
                    assert!(value.layout.items.iter().any(|item| matches!(item,
                        DisplayItem::SolidRect { rect, color, .. } if rect.y == 0.0 && color.blue == 255 && color.red == 0)));
                    // The protocol's separate coalescing regression preserves this owning
                    // revision. A timer presentation itself must not acquire its verdict.
                    assert!(
                        value.runtime.wheel_acknowledgements.is_empty(),
                        "the later blue reset snapshot must not own the wheel verdict"
                    );
                    session.shutdown().unwrap();
                    return;
                }
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document: value.document,
                        revision: value.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                pump_ready_task(&session, initial.document, value.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                assert_eq!(update.document, initial.document);
                assert!(update.runtime.wheel_acknowledgements.is_empty());
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                pump_ready_task(&session, initial.document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected reset verdict: {event:?}"),
        }
    }
    panic!("nested motion and its separate asynchronous reset snapshots were not observed");
}
