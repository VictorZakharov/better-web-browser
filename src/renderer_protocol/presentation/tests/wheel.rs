use super::*;
use crate::renderer_protocol::{MAX_WHEEL_ACKNOWLEDGEMENTS, WheelAcknowledgement, WheelDecision};

fn ack(sequence: u64, decision: WheelDecision) -> WheelAcknowledgement {
    WheelAcknowledgement {
        sequence,
        decision,
        viewport_delta_y: if decision == WheelDecision::Viewport {
            126.0
        } else {
            0.0
        },
        dispatch_micros: 200_000,
    }
}

#[test]
fn all_default_action_verdicts_round_trip_in_input_order() {
    let mut presentation = sample();
    presentation.runtime.wheel_acknowledgements = [
        WheelDecision::Cancelled,
        WheelDecision::NestedScroll,
        WheelDecision::Viewport,
        WheelDecision::NoMotion,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, decision)| ack(i as u64 + 1, decision))
    .collect();
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    assert_eq!(
        decoded.runtime.wheel_acknowledgements,
        presentation.runtime.wheel_acknowledgements
    );
    for invalid in [
        vec![ack(0, WheelDecision::Viewport)],
        vec![
            ack(1, WheelDecision::Viewport),
            ack(1, WheelDecision::Cancelled),
        ],
        vec![
            ack(2, WheelDecision::Viewport),
            ack(1, WheelDecision::Viewport),
        ],
        vec![ack(1, WheelDecision::Viewport); MAX_WHEEL_ACKNOWLEDGEMENTS + 1],
    ] {
        presentation.runtime.wheel_acknowledgements = invalid;
        assert!(presentation.encode().is_err());
    }
}

#[test]
fn coalescing_delivers_reversed_verdicts_separately_without_cancelling_their_distance() {
    let mut first = sample();
    first.runtime.viewport_wheel_delta_y = 126.0;
    first
        .runtime
        .wheel_acknowledgements
        .push(ack(1, WheelDecision::Viewport));
    let mut next = sample();
    next.revision = 2;
    next.runtime.viewport_wheel_delta_y = -126.0;
    let reverse = WheelAcknowledgement {
        viewport_delta_y: -126.0,
        ..ack(2, WheelDecision::Viewport)
    };
    next.runtime.wheel_acknowledgements.push(reverse);
    let (first, remainder) = first.coalesce(next).unwrap();
    let next = remainder.expect("opposite wheel must not be netted against the old target");
    assert_eq!(first.runtime.viewport_wheel_delta_y, 126.0);
    assert_eq!(next.runtime.viewport_wheel_delta_y, -126.0);
    assert_eq!(
        first.runtime.wheel_acknowledgements,
        [ack(1, WheelDecision::Viewport)]
    );
    assert_eq!(next.runtime.wheel_acknowledgements, [reverse]);
}

#[test]
fn coalescing_refuses_verdict_overflow_without_losing_reports() {
    let mut first = sample();
    first.runtime.wheel_acknowledgements = (1..=MAX_WHEEL_ACKNOWLEDGEMENTS as u64)
        .map(|sequence| ack(sequence, WheelDecision::Cancelled))
        .collect();
    let mut next = sample();
    next.revision = 2;
    next.runtime.wheel_acknowledgements.push(ack(
        MAX_WHEEL_ACKNOWLEDGEMENTS as u64 + 1,
        WheelDecision::NoMotion,
    ));
    let (first, next) = first.coalesce(next).unwrap();
    assert_eq!(
        first.runtime.wheel_acknowledgements.len(),
        MAX_WHEEL_ACKNOWLEDGEMENTS
    );
    assert_eq!(next.unwrap().runtime.wheel_acknowledgements.len(), 1);
}

#[test]
fn malformed_wire_verdict_tag_and_order_are_rejected() {
    use crate::renderer_protocol::wire::{WireReader, WireWriter};
    for (sequence, tag) in [(0, 1), (1, 0), (1, 5)] {
        let mut writer = WireWriter::new();
        writer.u32(1);
        writer.u64(sequence);
        writer.u8(tag);
        writer.f32(0.0);
        writer.u64(0);
        assert!(
            crate::renderer_protocol::presentation::wheel::decode(&mut WireReader::new(
                &writer.finish()
            ))
            .is_err()
        );
    }
}

#[test]
fn nested_verdict_snapshot_is_a_barrier_to_unrelated_resource_snapshots() {
    let mut first = sample();
    first
        .runtime
        .wheel_acknowledgements
        .push(ack(7, WheelDecision::NestedScroll));
    let mut resource = sample();
    resource.revision = 2;
    let (first, following) = first.coalesce(resource).unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(first.runtime.wheel_acknowledgements[0].sequence, 7);
    assert_eq!(following.unwrap().revision, 2);
}

#[test]
fn per_input_viewport_distance_survives_compaction_and_wire_round_trip() {
    let mut first = sample();
    first.runtime.viewport_wheel_delta_y = 0.25;
    let fractional = WheelAcknowledgement {
        viewport_delta_y: 0.25,
        ..ack(1, WheelDecision::Viewport)
    };
    first.runtime.wheel_acknowledgements.push(fractional);
    let mut next = sample();
    next.revision = 2;
    next.runtime.viewport_wheel_delta_y = 125.75;
    let following = WheelAcknowledgement {
        viewport_delta_y: 125.75,
        ..ack(3, WheelDecision::Viewport)
    };
    next.runtime.wheel_acknowledgements = vec![ack(2, WheelDecision::Cancelled), following];
    let (merged, remaining) = first.coalesce(next).unwrap();
    assert!(remaining.is_none());
    assert_eq!(merged.runtime.viewport_wheel_delta_y, 126.0);
    let decoded = RendererPresentation::decode(&merged.encode().unwrap()).unwrap();
    assert_eq!(
        decoded.runtime.wheel_acknowledgements,
        [fractional, ack(2, WheelDecision::Cancelled), following]
    );
}

#[test]
fn absolute_scroll_clears_only_prior_acknowledged_viewport_contributions() {
    let report = |sequence, delta, position| RuntimeReport {
        viewport_scroll_y: position,
        viewport_wheel_delta_y: delta,
        wheel_acknowledgements: vec![WheelAcknowledgement {
            viewport_delta_y: delta,
            ..ack(sequence, WheelDecision::Viewport)
        }],
        ..RuntimeReport::default()
    };
    let anchored = report(1, 126.0, None)
        .coalesce(report(2, -3.25, Some(500.0)))
        .unwrap();
    assert_eq!(anchored.viewport_scroll_y, Some(500.0));
    assert_eq!(anchored.viewport_wheel_delta_y, -3.25);
    assert_eq!(
        anchored.wheel_acknowledgements[0].decision,
        WheelDecision::Viewport
    );
    assert_eq!(anchored.wheel_acknowledgements[0].viewport_delta_y, 0.0);
    assert_eq!(anchored.wheel_acknowledgements[1].viewport_delta_y, -3.25);
    // Carrying the old anchor through another relative report must not clear
    // motion accepted after that anchor. Only an incoming absolute is a barrier.
    let combined = anchored.coalesce(report(3, 4.75, None)).unwrap();
    assert_eq!(combined.viewport_scroll_y, Some(500.0));
    assert_eq!(combined.viewport_wheel_delta_y, 1.5);
    assert_eq!(
        combined
            .wheel_acknowledgements
            .iter()
            .map(|value| value.viewport_delta_y)
            .collect::<Vec<_>>(),
        [0.0, -3.25, 4.75]
    );
    let mut presentation = sample();
    presentation.runtime = combined;
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    assert_eq!(decoded.runtime, presentation.runtime);
}

#[test]
fn acknowledgement_delta_validation_rejects_nonfinite_and_nonviewport_motion() {
    use crate::renderer_protocol::wire::{WireReader, WireWriter};
    for (decision, tag) in [
        (WheelDecision::Cancelled, 1),
        (WheelDecision::NestedScroll, 2),
        (WheelDecision::Viewport, 3),
        (WheelDecision::NoMotion, 4),
    ] {
        for delta in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0, -1.0] {
            if decision == WheelDecision::Viewport && delta.is_finite() {
                continue;
            }
            let mut presentation = sample();
            presentation.runtime.wheel_acknowledgements = vec![WheelAcknowledgement {
                viewport_delta_y: delta,
                ..ack(1, decision)
            }];
            assert!(presentation.encode().is_err(), "{decision:?} {delta}");
            assert!(
                presentation
                    .runtime
                    .clone()
                    .coalesce(RuntimeReport {
                        viewport_scroll_y: Some(0.0),
                        ..RuntimeReport::default()
                    })
                    .is_err(),
                "an absolute barrier must not sanitize invalid prior metadata"
            );
            let mut writer = WireWriter::new();
            writer.u32(1);
            writer.u64(1);
            writer.u8(tag);
            writer.f32(delta);
            writer.u64(0);
            assert!(
                crate::renderer_protocol::presentation::wheel::decode(&mut WireReader::new(
                    &writer.finish()
                ))
                .is_err(),
                "{decision:?} {delta}"
            );
        }
    }
}
