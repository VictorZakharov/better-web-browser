use super::*;
use crate::navigation::request::{FormPost, NavigationOptions};
use crate::renderer_protocol::presentation::runtime_codec::encode_runtime;
use crate::renderer_protocol::presentation::tests::sample;
use crate::renderer_protocol::wire::WireWriter;
use crate::renderer_protocol::{DocumentId, HistoryAction, MediaRuntimeReport, PageLoadReport};

fn encoded_runtime_len(report: &RuntimeReport) -> usize {
    let mut writer = WireWriter::new();
    encode_runtime(&mut writer, report).unwrap();
    writer.finish().len()
}

fn update(runtime: RuntimeReport) -> RendererRuntimeUpdate {
    RendererRuntimeUpdate {
        document: DocumentId::new(1).unwrap(),
        clock_advanced: true,
        runtime,
        load: PageLoadReport::default(),
        next_timer_micros: Some(10),
    }
}

fn detailed_report() -> RuntimeReport {
    RuntimeReport {
        errors: vec!["error".into()],
        console: vec!["console".into(), "Unicode: 🦋".into()],
        diagnostics: vec!["diagnostic".into()],
        history_actions: vec![
            HistoryAction::Update {
                url: "https://example.test/state".into(),
                replace: true,
                state: Some("{\"value\":1}".into()),
                scroll_y: 40.5,
            },
            HistoryAction::Traverse { delta: -1 },
        ],
        history_traversal_ack: Some(7),
        cookie_updates: vec!["name=value".into()],
        navigation_url: Some("https://example.test/post".into()),
        navigation_options: NavigationOptions {
            target: "_self".into(),
            post: Some(FormPost {
                content_type: "text/plain".into(),
                body: vec![1, 2, 3],
            }),
            ..NavigationOptions::default()
        },
        viewport_scroll_y: Some(20.0),
        media: Some(MediaRuntimeReport {
            backend: "decoder".into(),
            mime_type: "video/mp4".into(),
            video_codec: "avc1".into(),
            audio_codec: "mp4a".into(),
            failure: Some("failure".into()),
            ..MediaRuntimeReport::default()
        }),
        ..RuntimeReport::default()
    }
}

#[test]
fn read_only_sizing_matches_wire_encoding_including_optional_fields() {
    let report = detailed_report();
    for value in [RuntimeReport::default(), report.clone()] {
        assert_eq!(runtime_bytes(&value), encoded_runtime_len(&value));
    }
    for next in [RuntimeReport::default(), report.clone()] {
        let expected = merged_runtime_bytes(&report, &next).unwrap();
        let merged = report.clone().coalesce(next).unwrap();
        assert_eq!(expected, encoded_runtime_len(&merged));
    }
    let value = update(report);
    let mut writer = WireWriter::new();
    writer.u64(value.document.get());
    writer.bool(value.clock_advanced);
    encode_runtime(&mut writer, &value.runtime).unwrap();
    crate::renderer_protocol::presentation::codec::encode_load(&mut writer, value.load);
    writer.bool(true);
    writer.u64(10);
    assert_eq!(value.one_shot_resource_bytes(), writer.finish().len());
}

#[test]
fn every_edge_vector_keeps_original_reports_when_its_count_limit_would_be_crossed() {
    for field in 0..5 {
        let make = |count| {
            let mut report = RuntimeReport::default();
            let values = vec!["entry".into(); count];
            match field {
                0 => report.errors = values,
                1 => report.console = values,
                2 => report.diagnostics = values,
                3 => report.cookie_updates = values,
                _ => {
                    report.history_actions = values
                        .into_iter()
                        .map(|url| HistoryAction::Update {
                            url,
                            replace: false,
                            state: None,
                            scroll_y: 0.0,
                        })
                        .collect()
                }
            }
            report
        };
        let first = update(make(MAX_RUNTIME_REPORT_ENTRIES));
        let next = update(make(1));
        let (retained, remaining) = first.clone().coalesce(next.clone()).unwrap();
        assert_eq!(retained, first);
        assert_eq!(remaining, Some(next));

        let mut a = sample();
        a.runtime = make(MAX_RUNTIME_REPORT_ENTRIES);
        let mut b = sample();
        b.revision = 2;
        b.runtime = make(1);
        let (retained, remaining) = a.coalesce(b).unwrap();
        assert_eq!(retained.revision, 1);
        assert_eq!(remaining.unwrap().revision, 2);
    }
}

#[test]
fn runtime_control_payload_budget_preserves_valid_updates_as_separate_chunks() {
    let first = update(RuntimeReport {
        console: vec!["a".repeat(64 * 1024); 3],
        ..RuntimeReport::default()
    });
    let next = update(RuntimeReport {
        diagnostics: vec!["b".repeat(64 * 1024); 3],
        ..RuntimeReport::default()
    });
    assert!(first.one_shot_resource_bytes() < MAX_CONTROL_PAYLOAD);
    assert!(next.one_shot_resource_bytes() < MAX_CONTROL_PAYLOAD);
    let (retained, remaining) = first.clone().coalesce(next.clone()).unwrap();
    assert_eq!(retained, first);
    assert_eq!(remaining, Some(next));
}

#[test]
fn presentation_union_budget_includes_runtime_text_beside_bitmap_bytes() {
    use crate::engine::DecodedImage;
    use crate::renderer_protocol::PresentedImage;
    let mut first = sample();
    first.glyphs.clear();
    first.runtime = RuntimeReport {
        console: vec!["a".repeat(64 * 1024); 32],
        ..RuntimeReport::default()
    };
    let mut next = sample();
    next.revision = 2;
    next.glyphs.clear();
    let pixels: std::sync::Arc<[u8]> = vec![0; 1024 * 1024].into();
    next.images = (0..63)
        .map(|index| PresentedImage {
            url: format!("canvas:{index}"),
            image: DecodedImage {
                width: 512,
                height: 512,
                bgra: pixels.clone(),
            },
        })
        .collect();
    assert!(resources::merged_bytes(&first, &next).is_some());
    let (retained, remaining) = first.coalesce(next).unwrap();
    assert_eq!(retained.revision, 1);
    assert_eq!(remaining.unwrap().revision, 2);
}
