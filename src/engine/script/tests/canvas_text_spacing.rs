use super::*;

const SPACING: &str = include_str!("../../../../tests/canvas/text-spacing.js");

#[test]
fn canvas_text_spacing_changes_measurement_fill_and_stroke_and_survives_state_stack() {
    let (_, outcome) = execute_html(&format!(
        "<script>{SPACING}\nconst make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};testCanvasTextSpacing(make);</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_canvas_text_spacing_uses_real_shaping_in_its_own_realm() {
    let source = format!(
        "{SPACING}\nconst make=(w,h)=>new OffscreenCanvas(w,h);testCanvasTextSpacing(make);postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-text-spacing.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
