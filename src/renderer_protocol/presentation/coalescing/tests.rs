use super::*;
use crate::engine::DecodedImage;
use crate::renderer_protocol::HistoryAction;
use crate::renderer_protocol::PresentedImage;
use crate::renderer_protocol::presentation::tests::sample;

fn merged(first: RendererPresentation, next: RendererPresentation) -> RendererPresentation {
    let (value, remaining) = first.coalesce(next).unwrap();
    assert!(remaining.is_none(), "small presentations should coalesce");
    value
}

#[test]
fn coalesced_scroll_requests_keep_the_latest_offset_including_zero() {
    let first = RuntimeReport {
        viewport_scroll_y: Some(500.0),
        ..RuntimeReport::default()
    };
    let retained = first.coalesce(RuntimeReport::default()).unwrap();
    assert_eq!(retained.viewport_scroll_y, Some(500.0));
    let final_report = retained
        .coalesce(RuntimeReport {
            viewport_scroll_y: Some(0.0),
            ..RuntimeReport::default()
        })
        .unwrap();
    assert_eq!(final_report.viewport_scroll_y, Some(0.0));
}

#[test]
fn wheel_deltas_accumulate_until_an_absolute_scroll_supersedes_them() {
    let wheel = |delta| RuntimeReport {
        viewport_wheel_delta_y: delta,
        ..RuntimeReport::default()
    };
    let combined = wheel(126.0)
        .coalesce(wheel(126.0))
        .unwrap()
        .coalesce(wheel(-40.0))
        .unwrap();
    assert_eq!(combined.viewport_wheel_delta_y, 212.0);
    let repositioned = combined
        .coalesce(RuntimeReport {
            viewport_scroll_y: Some(30.0),
            viewport_wheel_delta_y: 5.0,
            ..RuntimeReport::default()
        })
        .unwrap();
    assert_eq!(repositioned.viewport_wheel_delta_y, 5.0);
    let next = repositioned.coalesce(wheel(20.0)).unwrap();
    assert_eq!(next.viewport_scroll_y, Some(30.0));
    assert_eq!(next.viewport_wheel_delta_y, 25.0);
    assert!(
        wheel(f32::MAX)
            .coalesce(wheel(f32::MAX))
            .unwrap()
            .viewport_wheel_delta_y
            .is_finite()
    );
}

#[test]
fn coalescing_rejects_an_unbounded_history_action_stream() {
    let action = HistoryAction::Traverse { delta: -1 };
    let first = RuntimeReport {
        history_actions: vec![action.clone(); MAX_RUNTIME_REPORT_ENTRIES],
        ..Default::default()
    };
    let next = RuntimeReport {
        history_actions: vec![action],
        ..Default::default()
    };
    assert!(matches!(
        first.coalesce(next),
        Err(ProtocolError::InvalidPayload(
            "coalesced runtime entry count"
        ))
    ));
}

#[test]
fn preserves_ordered_deltas_and_one_shot_resources() {
    let mut first = sample();
    first.clock_advanced = true;
    first.runtime = RuntimeReport {
        scripts_executed: 2,
        dom_mutations: 3,
        errors: vec!["first error".into()],
        console: vec!["first console".into()],
        diagnostics: vec!["first diagnostic".into()],
        navigation_url: Some("https://example.test/redirect".into()),
        history_actions: vec![HistoryAction::Update {
            url: "https://example.test/first-state".into(),
            replace: false,
            state: None,
            scroll_y: 0.0,
        }],
        cookie_updates: vec!["first=1".into()],
        runtime_active: true,
        render_requested: true,
        ..RuntimeReport::default()
    };
    first.style.invalidated_nodes = 2;
    first.style.total_styles = 3;
    first.load.parse_micros = 5;
    first.load.text_measure_count = 6;
    first.load.text_shape_cache_entries = 7;
    first.images.push(PresentedImage {
        url: "https://example.test/first.png".into(),
        image: DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![1; 4].into(),
        },
    });

    let mut next = sample();
    next.revision = 2;
    next.title = "newest snapshot".into();
    next.runtime = RuntimeReport {
        scripts_executed: 4,
        dom_mutations: 5,
        errors: vec!["next error".into()],
        console: vec!["next console".into()],
        diagnostics: vec!["next diagnostic".into()],
        history_actions: vec![HistoryAction::Update {
            url: "https://example.test/next-state".into(),
            replace: true,
            state: None,
            scroll_y: 0.0,
        }],
        cookie_updates: vec!["next=2".into()],
        runtime_stopped: true,
        ..RuntimeReport::default()
    };
    next.style.invalidated_nodes = 4;
    next.style.total_styles = 6;
    next.load.parse_micros = 9;
    next.load.text_measure_count = 10;
    next.load.text_shape_cache_entries = 11;
    next.images.push(PresentedImage {
        url: "https://example.test/next.png".into(),
        image: DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![2; 4].into(),
        },
    });
    next.glyphs[0].id = 2;

    let combined = merged(first, next);
    assert_eq!(combined.revision, 2);
    assert!(combined.clock_advanced);
    assert_eq!(combined.title, "newest snapshot");
    assert_eq!(combined.runtime.scripts_executed, 6);
    assert_eq!(combined.runtime.dom_mutations, 8);
    assert_eq!(combined.runtime.errors, ["first error", "next error"]);
    assert_eq!(combined.runtime.console, ["first console", "next console"]);
    assert_eq!(
        combined.runtime.diagnostics,
        ["first diagnostic", "next diagnostic"]
    );
    assert_eq!(combined.runtime.cookie_updates, ["first=1", "next=2"]);
    assert_eq!(
        combined.runtime.navigation_url.as_deref(),
        Some("https://example.test/redirect")
    );
    assert_eq!(
        combined.runtime.history_actions,
        [
            HistoryAction::Update {
                url: "https://example.test/first-state".into(),
                replace: false,
                state: None,
                scroll_y: 0.0,
            },
            HistoryAction::Update {
                url: "https://example.test/next-state".into(),
                replace: true,
                state: None,
                scroll_y: 0.0,
            }
        ]
    );
    assert!(!combined.runtime.runtime_active);
    assert!(combined.runtime.runtime_stopped);
    assert!(combined.runtime.render_requested);
    assert_eq!(combined.style.invalidated_nodes, 6);
    assert_eq!(combined.style.total_styles, 9);
    assert_eq!(combined.load.parse_micros, 14);
    assert_eq!(combined.load.text_measure_count, 16);
    assert_eq!(combined.load.text_shape_cache_entries, 11);
    assert_eq!(
        combined
            .images
            .iter()
            .map(|image| image.url.as_str())
            .collect::<Vec<_>>(),
        [
            "https://example.test/first.png",
            "https://example.test/next.png"
        ]
    );
    assert_eq!(
        combined
            .glyphs
            .iter()
            .map(|glyph| glyph.id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn drops_glyphs_from_an_obsolete_epoch() {
    let first = sample();
    let mut next = sample();
    next.revision = 2;
    next.glyph_epoch = 2;
    next.glyphs[0].id = 2;

    let combined = merged(first, next);
    assert_eq!(combined.glyph_epoch, 2);
    assert_eq!(combined.glyphs.len(), 1);
    assert_eq!(combined.glyphs[0].id, 2);
}

#[test]
fn coalesced_image_retirement_obeys_later_update_order() {
    let key = "breeze-internal:canvas:7";
    let bitmap = PresentedImage {
        url: key.into(),
        image: DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![1; 4].into(),
        },
    };
    let mut first = sample();
    first.images.push(bitmap.clone());
    let mut removal = sample();
    removal.revision = 2;
    removal.retired_image_keys.push(key.into());
    let retired = merged(first, removal);
    assert!(retired.images.is_empty());
    assert_eq!(retired.retired_image_keys, [key]);

    let mut replacement = sample();
    replacement.revision = 3;
    replacement.images.push(bitmap);
    let restored = merged(retired, replacement);
    assert!(restored.retired_image_keys.is_empty());
    assert_eq!(restored.images.len(), 1);
    assert_eq!(restored.images[0].url, key);
}
