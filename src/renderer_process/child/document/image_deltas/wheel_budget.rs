//! Wheel metadata must be present before decoded images consume wire capacity.
use super::*;
use crate::engine::{DecodedImage, RectF, ScriptOutcome};
use crate::renderer_process::child::document::reporting::{
    runtime_report, runtime_report_with_wheel,
};
use crate::renderer_protocol::{
    AccessibilityUpdate, DocumentId, DocumentNodeId, PresentedLayout, ProtocolError,
    WheelAcknowledgement, WheelDecision,
};

fn acknowledgement() -> WheelAcknowledgement {
    WheelAcknowledgement {
        sequence: 1,
        decision: WheelDecision::NestedScroll,
        viewport_delta_y: 0.0,
        dispatch_micros: 25,
    }
}

fn presentation() -> RendererPresentation {
    RendererPresentation {
        document: DocumentId::new(1).unwrap(),
        revision: 1,
        clock_advanced: false,
        title: String::new(),
        final_url: "about:blank".into(),
        status: 200,
        character_set: "utf-8".into(),
        reader: crate::document::Document {
            title: String::new(),
            source_url: "about:blank".into(),
            blocks: Vec::new(),
            truncated: false,
        },
        layout: PresentedLayout::default(),
        images: Vec::new(),
        retired_image_keys: Vec::new(),
        glyph_epoch: 1,
        glyphs: Vec::new(),
        runtime: runtime_report(ScriptOutcome::default(), true, None),
        style: Default::default(),
        load: Default::default(),
        page_diagnostics: Default::default(),
        accessibility: AccessibilityUpdate::full_root(
            DocumentNodeId::new((1_u128 << 64) | 1).unwrap(),
            "",
            RectF::default(),
        ),
        next_timer_micros: Some(0),
    }
}

fn candidate(url: String, width: u32, height: u32) -> ImageDelta {
    ImageDelta {
        presented: PresentedImage {
            url,
            image: DecodedImage {
                width,
                height,
                bgra: vec![0; width as usize * height as usize * 4].into(),
            },
        },
        canvas_update: false,
        frame: None,
        already_sent: false,
    }
}

#[test]
fn actual_wheel_metadata_precedes_image_admission_at_the_exact_wire_boundary() {
    let mut value = presentation();
    let base = value.encode().unwrap().len();
    let actual_runtime = runtime_report_with_wheel(
        ScriptOutcome::default(),
        true,
        None,
        Some(acknowledgement()),
    );
    let mut metadata = value.clone();
    metadata.runtime = actual_runtime.clone();
    let with_wheel = metadata.encode().unwrap().len();
    let wheel_metadata_bytes = with_wheel - base;
    // Sequence, decision, retained CSS distance, and dispatch duration. Measure
    // the real codec as well so this boundary fixture cannot silently drift.
    assert_eq!(wheel_metadata_bytes, 8 + 1 + 4 + 8);
    let large = candidate("large".into(), 8192, 2047);
    let mut tail_url = String::from("tail");
    // Pixel payloads are multiples of four. URL padding makes this valid pair of
    // images fill the real 64 MiB presentation boundary exactly, not a mock budget.
    let padding =
        (MAX_RENDERER_PRESENTATION_BYTES - base - large.wire_bytes() - 16 - tail_url.len()) % 4;
    tail_url.push_str(&"x".repeat(padding));
    let tail_bytes =
        MAX_RENDERER_PRESENTATION_BYTES - base - large.wire_bytes() - 16 - tail_url.len();
    let tail = candidate(tail_url, (tail_bytes / 4) as u32, 1);
    let candidates = [large, tail];
    let old_selection = select(&candidates, MAX_RENDERER_PRESENTATION_BYTES - base);
    assert_eq!(old_selection.indexes, [0, 1]);
    value.images = candidates
        .iter()
        .map(|image| image.presented.clone())
        .collect();
    assert_eq!(
        value.encode().unwrap().len(),
        MAX_RENDERER_PRESENTATION_BYTES
    );

    value.runtime = actual_runtime;
    assert!(
        matches!(value.encode(), Err(ProtocolError::PayloadTooLarge(bytes))
        if bytes as usize == MAX_RENDERER_PRESENTATION_BYTES + wheel_metadata_bytes),
        "attaching after image admission must reproduce the exact old overflow"
    );
    value.images.clear();
    assert_eq!(value.encode().unwrap().len(), with_wheel);
    let bounded = select(&candidates, MAX_RENDERER_PRESENTATION_BYTES - with_wheel);
    assert_eq!(bounded.indexes, [0]);
    assert!(bounded.deferred);
    assert!(!bounded.unpresentable);
    value.images = bounded
        .indexes
        .iter()
        .map(|index| candidates[*index].presented.clone())
        .collect();
    assert!(value.encode().unwrap().len() <= MAX_RENDERER_PRESENTATION_BYTES);
    assert_eq!(value.runtime.wheel_acknowledgements, [acknowledgement()]);
    assert_eq!(value.next_timer_micros, Some(0));
}
