use super::*;
use better_web_browser::renderer_protocol::WheelAcknowledgement;

fn report(values: &[(u64, WheelDecision, f32)]) -> RuntimeReport {
    RuntimeReport {
        viewport_wheel_delta_y: values.iter().map(|(_, _, delta)| delta).sum(),
        wheel_acknowledgements: values
            .iter()
            .map(
                |&(sequence, decision, viewport_delta_y)| WheelAcknowledgement {
                    sequence,
                    decision,
                    viewport_delta_y,
                    dispatch_micros: 0,
                },
            )
            .collect(),
        ..RuntimeReport::default()
    }
}

#[test]
fn delayed_forward_replies_cannot_restart_after_physical_reversal() {
    for direction in [-1.0, 1.0] {
        let mut gesture = WheelGesture::default();
        for sequence in 1..=100_000 {
            assert!(!gesture.observe(sequence, direction * 126.0));
        }
        assert!(gesture.observe(100_001, -direction * 0.1));
        for sequence in [1, 8, 128, 100_000] {
            assert_eq!(
                gesture.viewport_delta(&report(&[(
                    sequence,
                    WheelDecision::Viewport,
                    direction * 126.0
                )])),
                0.0
            );
        }
        assert_eq!(
            gesture.viewport_delta(&report(&[(
                100_001,
                WheelDecision::Viewport,
                -direction * 0.1
            )])),
            -direction * 0.1
        );
    }
}

#[test]
fn a_second_reversal_filters_old_defaults_inside_a_compacted_report() {
    let mut gesture = WheelGesture::default();
    gesture.observe(1, 126.0);
    assert!(gesture.observe(2, -126.0));
    assert!(gesture.observe(3, 126.0));
    assert!(!gesture.observe(4, 126.0));
    let combined = report(&[
        (1, WheelDecision::Viewport, 126.0),
        (2, WheelDecision::Viewport, -126.0),
        (3, WheelDecision::Viewport, 126.0),
        (4, WheelDecision::Viewport, 126.0),
    ]);
    assert_eq!(gesture.viewport_delta(&combined), 252.0);
    assert_eq!(
        combined.wheel_acknowledgements.len(),
        4,
        "no DOM verdict is dropped"
    );
}

#[test]
fn cancelled_nested_or_rejected_reverse_still_retires_old_viewport_travel() {
    for decision in [
        WheelDecision::Cancelled,
        WheelDecision::NestedScroll,
        WheelDecision::NoMotion,
    ] {
        let mut gesture = WheelGesture::default();
        gesture.observe(1, 126.0);
        assert!(gesture.observe(2, -126.0));
        assert_eq!(
            gesture.viewport_delta(&report(&[(1, WheelDecision::Viewport, 126.0)])),
            0.0
        );
        let next = report(&[(2, decision, 0.0)]);
        assert_eq!(gesture.viewport_delta(&next), 0.0);
        assert_eq!(next.wheel_acknowledgements[0].decision, decision);
    }
    // A rejected reverse has no renderer acknowledgement at all. Its sequence
    // fence remains authoritative when an earlier accepted default arrives.
    let mut rejected = WheelGesture::default();
    rejected.observe(7, 126.0);
    rejected.observe(8, -126.0);
    assert_eq!(
        rejected.viewport_delta(&report(&[(7, WheelDecision::Viewport, 126.0)])),
        0.0
    );
}

#[test]
fn zero_horizontal_and_invalid_input_do_not_cancel_a_vertical_gesture() {
    let mut gesture = WheelGesture::default();
    gesture.observe(10, 126.0);
    for (sequence, delta) in [(11, 0.0), (12, f32::NAN), (13, f32::INFINITY), (0, -126.0)] {
        assert!(!gesture.observe(sequence, delta));
    }
    assert_eq!(gesture.first_live_sequence(), 10);
    assert_eq!(
        gesture.viewport_delta(&report(&[(10, WheelDecision::Viewport, 126.0)])),
        126.0
    );
    let fresh = WheelGesture::default();
    assert_eq!(
        fresh.first_live_sequence(),
        0,
        "a new document/tab owns fresh sequence state"
    );
}

#[test]
fn script_absolute_and_unowned_runtime_data_are_not_modified_by_the_filter() {
    let mut gesture = WheelGesture::default();
    gesture.observe(1, 126.0);
    gesture.observe(2, -126.0);
    let mut source = report(&[
        (1, WheelDecision::Viewport, 0.0),
        (2, WheelDecision::Viewport, -126.0),
    ]);
    source.viewport_scroll_y = Some(900.0);
    assert_eq!(gesture.viewport_delta(&source), -126.0);
    assert_eq!(source.viewport_scroll_y, Some(900.0));
    let quiet = RuntimeReport {
        viewport_scroll_y: Some(100.0),
        ..RuntimeReport::default()
    };
    assert_eq!(gesture.viewport_delta(&quiet), 0.0);
    assert_eq!(quiet.viewport_scroll_y, Some(100.0));
}

#[test]
fn tab_suspension_retires_pending_defaults_without_reusing_an_old_sequence() {
    let mut gesture = WheelGesture::default();
    gesture.observe(10, 126.0);
    gesture.retire_before(11);
    assert_eq!(
        gesture.viewport_delta(&report(&[(10, WheelDecision::Viewport, 126.0)])),
        0.0
    );
    gesture.retire_before(3);
    assert_eq!(gesture.first_live_sequence(), 11);
    assert!(!gesture.observe(11, 126.0));
    assert_eq!(
        gesture.viewport_delta(&report(&[(11, WheelDecision::Viewport, 126.0)])),
        126.0
    );
}
