use super::*;

const BINDINGS: &str = include_str!("../../../../tests/canvas/drawing-state-bindings.js");

#[test]
fn canvas_drawing_state_and_text_follow_idl_conversion_order() {
    let (_, outcome) = execute_html(&format!(
        "<script>{BINDINGS}\nconst make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};testCanvasDrawingStateConversions(make);testCanvasTextArgumentConversions(make);</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_canvas_drawing_state_and_text_follow_idl_conversion_order() {
    let source = format!(
        "{BINDINGS}\nconst make=(w,h)=>new OffscreenCanvas(w,h);testCanvasDrawingStateConversions(make);testCanvasTextArgumentConversions(make);postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-drawing-state-bindings.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
