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
