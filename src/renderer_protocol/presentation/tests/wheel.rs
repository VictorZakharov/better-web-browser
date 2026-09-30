use super::*;
use crate::renderer_protocol::{MAX_WHEEL_ACKNOWLEDGEMENTS, WheelAcknowledgement, WheelDecision};

fn ack(sequence: u64, decision: WheelDecision) -> WheelAcknowledgement {
    WheelAcknowledgement {
        sequence,
        decision,
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
fn coalescing_keeps_reversed_verdicts_and_refuses_overflow_without_losing_reports() {
    let mut first = sample();
    first.runtime.viewport_wheel_delta_y = 126.0;
    first
        .runtime
        .wheel_acknowledgements
        .push(ack(1, WheelDecision::Viewport));
    let mut next = sample();
    next.revision = 2;
    next.runtime.viewport_wheel_delta_y = -126.0;
    next.runtime
        .wheel_acknowledgements
        .push(ack(2, WheelDecision::Viewport));
    let (merged, remainder) = first.coalesce(next).unwrap();
    assert!(remainder.is_none());
    assert_eq!(merged.runtime.viewport_wheel_delta_y, 0.0);
    assert_eq!(
        merged
            .runtime
            .wheel_acknowledgements
            .iter()
            .map(|value| value.sequence)
            .collect::<Vec<_>>(),
        [1, 2]
    );
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
