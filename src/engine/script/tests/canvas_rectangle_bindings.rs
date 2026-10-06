use super::*;

const RECTANGLES: &str = include_str!("../../../../tests/canvas/rectangle-bindings.js");

#[test]
fn canvas_rectangles_brand_and_convert_before_drawing_or_layer_allocation() {
    let (_, outcome) = execute_html(&format!(
        "<script>{RECTANGLES}\ntestCanvasRectangleBindings((w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}});</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_canvas_rectangles_share_the_window_binding_contract() {
    let source = format!(
        "{RECTANGLES}\ntestCanvasRectangleBindings((w,h)=>new OffscreenCanvas(w,h));postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/rectangle-bindings.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
