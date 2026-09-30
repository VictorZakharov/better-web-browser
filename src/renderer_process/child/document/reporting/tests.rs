use super::*;
use crate::limits::{MAX_RUNTIME_REPORT_ENTRIES, MAX_RUNTIME_REPORT_TEXT_BYTES};
use crate::renderer_protocol::{
    DocumentId, FrameReader, FrameWriter, RendererMessage, RendererRuntimeUpdate, RendererSessionId,
};
use std::io::Cursor;

fn round_trip(report: RuntimeReport) {
    let session = RendererSessionId::new(1).unwrap();
    let message = RendererMessage::RuntimeUpdate(Box::new(RendererRuntimeUpdate {
        document: DocumentId::new(1).unwrap(),
        clock_advanced: false,
        runtime: report,
        load: Default::default(),
        next_timer_micros: None,
    }));
    let mut writer = FrameWriter::new(Vec::new(), session);
    writer.send_renderer(&message).unwrap();
    let mut reader = FrameReader::new(Cursor::new(writer.into_inner()), session);
    assert_eq!(reader.read_renderer().unwrap(), message);
}

#[test]
fn runtime_reporting_bounds_telemetry_counts_without_losing_state_updates() {
    let entries: Vec<_> = (0..700).map(|i| format!("log {i}")).collect();
    let report = runtime_report(
        ScriptOutcome {
            errors: entries.clone(),
            console: entries.clone(),
            diagnostics: entries,
            cookie_updates: vec!["a=1".into(), "a=2".into()],
            navigation_url: Some("https://example.test/next".into()),
            ..Default::default()
        },
        true,
        None,
    );
    for lane in [&report.errors, &report.console, &report.diagnostics] {
        assert!(lane.len() <= MAX_RUNTIME_REPORT_ENTRIES);
        assert_eq!(lane[0], "log 0");
        assert!(lane.last().unwrap().contains("omitted"));
    }
    assert_eq!(report.cookie_updates, ["a=1", "a=2"]);
    assert_eq!(
        report.navigation_url.as_deref(),
        Some("https://example.test/next")
    );
    assert!(report.runtime_active);
    round_trip(report);
}

#[test]
fn runtime_reporting_bounds_unicode_and_aggregate_text_without_silent_truncation() {
    let report = runtime_report(
        ScriptOutcome {
            console: vec!["\u{1f600}".repeat(MAX_RUNTIME_REPORT_TEXT_BYTES)],
            diagnostics: vec!["entry".repeat(2000); 32],
            ..Default::default()
        },
        true,
        None,
    );
    for lane in [&report.console, &report.diagnostics] {
        assert!(lane.iter().map(String::len).sum::<usize>() <= MAX_RUNTIME_REPORT_TEXT_BYTES);
        assert!(lane.last().unwrap().contains("truncated"));
    }
    round_trip(report);
}

#[test]
fn runtime_reporting_leaves_small_diagnostics_unchanged() {
    let report = runtime_report(
        ScriptOutcome {
            console: vec!["one".into(), "two".into()],
            ..Default::default()
        },
        true,
        None,
    );
    assert_eq!(report.console, ["one", "two"]);
    assert!(report.errors.is_empty());
    round_trip(report);
}

#[test]
fn quiet_and_cancelled_wheel_reports_include_actual_metadata_before_encoding() {
    use crate::renderer_protocol::{WheelAcknowledgement, WheelDecision};
    for decision in [WheelDecision::Cancelled, WheelDecision::NoMotion] {
        let acknowledgement = WheelAcknowledgement {
            sequence: 1,
            decision,
            dispatch_micros: 25,
        };
        let report =
            runtime_report_with_wheel(ScriptOutcome::default(), true, None, Some(acknowledgement));
        assert_eq!(report.wheel_acknowledgements, [acknowledgement]);
        round_trip(report);
    }
}

#[test]
fn combining_script_tasks_keeps_native_value_snapshots_within_the_shared_budget() {
    use crate::engine::dom::NodeId;
    use crate::engine::script::ScriptSelectionAction;
    use crate::renderer_protocol::TextSelectionDirection;
    let root = NodeId::from_wire((1_u128 << 64) | 1).unwrap();
    let action = |index| ScriptSelectionAction {
        node: NodeId::from_wire((1_u128 << 64) | index).unwrap(),
        value: "x".repeat(crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES / 2),
        selection_start: 0,
        selection_end: 0,
        direction: TextSelectionDirection::None,
    };
    let mut combined = ScriptOutcome {
        selection_actions: vec![action(2), action(3)],
        ..Default::default()
    };
    merge_outcome(
        &mut combined,
        ScriptOutcome {
            selection_actions: vec![action(4)],
            ..Default::default()
        },
        root,
    );
    assert_eq!(combined.selection_actions.len(), 2);
    assert_eq!(combined.selection_actions[0].node, action(3).node);
    assert_eq!(combined.selection_actions[1].node, action(4).node);
    assert_eq!(
        combined
            .selection_actions
            .iter()
            .map(|action| action.value.len())
            .sum::<usize>(),
        crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES
    );
}
