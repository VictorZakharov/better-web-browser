use super::*;

fn document(id: u64) -> DocumentId {
    DocumentId::new(id).unwrap()
}
fn ack(sequence: u64, decision: WheelDecision) -> WheelAcknowledgement {
    WheelAcknowledgement {
        sequence,
        decision,
        dispatch_micros: 10_000,
    }
}

#[test]
fn renderer_queue_stall_and_dispatch_work_are_separate_from_first_motion_paint() {
    let now = Instant::now();
    let mut trace = WheelTrace::default();
    trace.enqueue(document(1), 1, 126, now);
    trace.acknowledge(
        document(1),
        &[ack(1, WheelDecision::Viewport)],
        None,
        now + Duration::from_millis(250),
    );
    trace.painted(document(1), None, now + Duration::from_millis(265), false);
    let sample = &trace.samples[0];
    assert_eq!(sample.decision_received, Some(Duration::from_millis(250)));
    assert_eq!(sample.dispatch, Some(Duration::from_millis(10)));
    assert_eq!(sample.first_paint, Some(Duration::from_millis(265)));
    assert_eq!(sample.status, "painted");
}

#[test]
fn cancelled_zero_motion_and_unacknowledged_inputs_are_never_reported_as_painted() {
    let now = Instant::now();
    let mut trace = WheelTrace::default();
    for sequence in 1..=3 {
        trace.enqueue(document(1), sequence, 126, now);
    }
    trace.acknowledge(
        document(1),
        &[
            ack(1, WheelDecision::Cancelled),
            ack(2, WheelDecision::NoMotion),
        ],
        None,
        now,
    );
    trace.painted(document(1), None, now + Duration::from_millis(10), false);
    assert_eq!(
        trace
            .samples
            .iter()
            .map(|sample| sample.status)
            .collect::<Vec<_>>(),
        ["cancelled", "no_motion", "unacknowledged"]
    );
    assert!(
        trace
            .samples
            .iter()
            .all(|sample| sample.first_paint.is_none())
    );
    let json: serde_json::Value = serde_json::from_str(&trace.to_json()).unwrap();
    assert!(
        json["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|sample| sample["enqueue_to_first_paint_ms"].is_null())
    );
}

#[test]
fn nested_paint_requires_its_installation_not_a_later_resource_or_async_reset_snapshot() {
    let now = Instant::now();
    let mut trace = WheelTrace::default();
    trace.enqueue(document(1), 1, 75, now);
    trace.acknowledge(
        document(1),
        &[ack(1, WheelDecision::NestedScroll)],
        Some(7),
        now,
    );
    trace.painted(document(1), None, now, false);
    trace.painted(document(1), Some(8), now, false);
    trace.painted(document(2), Some(7), now, false);
    assert!(trace.samples[0].first_paint.is_none());
    trace.painted(document(1), Some(7), now + Duration::from_millis(25), true);
    assert_eq!(
        trace.samples[0].first_paint,
        Some(Duration::from_millis(25))
    );
    // A later scroll-listener reset/color update has no wheel verdict. It can neither
    // establish the earlier input's first paint nor overwrite an already owned paint.
    trace.acknowledge(document(1), &[], Some(8), now + Duration::from_millis(30));
    trace.painted(document(1), Some(8), now + Duration::from_millis(35), true);
    assert_eq!(trace.samples[0].nested_revision, Some(7));
    assert_eq!(
        trace.samples[0].first_paint,
        Some(Duration::from_millis(25))
    );
    assert_eq!(trace.samples[0].paint_path, Some("full_retained"));
}

#[test]
fn reversed_coalesced_deltas_keep_both_verdicts_without_inventing_two_paints() {
    let now = Instant::now();
    let mut trace = WheelTrace::default();
    trace.enqueue(document(1), 1, 126, now);
    trace.enqueue(document(1), 2, -126, now);
    trace.acknowledge(
        document(1),
        &[
            ack(1, WheelDecision::Viewport),
            ack(2, WheelDecision::Viewport),
        ],
        None,
        now,
    );
    trace.no_motion(document(1), None);
    trace.painted(document(1), None, now, false);
    assert_eq!(trace.samples[0].status, "superseded");
    assert_eq!(trace.samples[1].status, "no_motion");
    assert!(
        trace
            .samples
            .iter()
            .all(|sample| sample.first_paint.is_none())
    );
}

#[test]
fn zero_net_fractional_and_clamped_requests_cannot_claim_existing_animation_ticks() {
    let now = Instant::now();
    for case in [
        "zero",
        "coalesced_opposites",
        "fractional",
        "clamped",
        "cancel_to_current",
    ] {
        let mut trace = WheelTrace::default();
        trace.enqueue(document(1), 1, 600, now);
        trace.acknowledge(document(1), &[ack(1, WheelDecision::Viewport)], None, now);
        trace.painted(document(1), None, now + Duration::from_millis(10), false);
        trace.enqueue(document(1), 2, 600, now + Duration::from_millis(15));
        trace.acknowledge(
            document(1),
            &[ack(2, WheelDecision::Viewport)],
            None,
            now + Duration::from_millis(20),
        );
        // The native owner resolves this BEFORE its synchronous tick, not after a
        // returned boolean could already have credited pixels from the old target.
        trace.viewport_request(document(1), false);
        trace.painted(document(1), None, now + Duration::from_millis(21), false);
        trace.painted(document(1), None, now + Duration::from_millis(40), false);
        assert_eq!(
            trace.samples[0].first_paint,
            Some(Duration::from_millis(10)),
            "{case}"
        );
        assert_eq!(trace.samples[1].status, "no_motion", "{case}");
        assert!(trace.samples[1].first_paint.is_none(), "{case}");
        assert!(trace.samples[1].paint_path.is_none(), "{case}");
    }
}

#[test]
fn stale_document_and_sequence_cannot_complete_new_inputs_and_capacity_is_visible() {
    let now = Instant::now();
    let mut trace = WheelTrace::default();
    trace.enqueue(document(1), 1, 126, now);
    trace.retire_document(document(1));
    trace.enqueue(document(2), 1, 126, now);
    trace.acknowledge(document(1), &[ack(1, WheelDecision::Viewport)], None, now);
    trace.painted(document(1), None, now, false);
    assert!(trace.samples[0].first_paint.is_none());
    // A stale report is filtered by navigation ownership before the trace hook.
    assert_eq!(trace.samples[0].status, "retired_document");
    assert_eq!(trace.samples[1].status, "unacknowledged");
    assert_eq!(trace.unmatched_acknowledgements, 1);
    for sequence in 2..=MAX_SAMPLES as u64 + 1 {
        trace.enqueue(document(2), sequence, 126, now);
    }
    assert_eq!(trace.samples.len(), MAX_SAMPLES);
    assert_eq!(trace.omitted_inputs, 2);
}
