use super::*;

const PRIVATE: &str = include_str!("../../../../tests/canvas/private-drawing-state.js");

#[test]
fn canvas_renderer_state_and_reset_helpers_are_not_author_properties() {
    let (_, outcome) = execute_html(&format!(
        "<script>{PRIVATE}\nconst make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};testPrivateCanvasDrawingState(make);testCanvasContextReceiverOrder(make);testCanvasPrivateSerialization(make);</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_canvas_renderer_state_and_receiver_order_are_private() {
    let source = format!(
        "{PRIVATE}\nconst make=(w,h)=>new OffscreenCanvas(w,h);testPrivateCanvasDrawingState(make);testCanvasContextReceiverOrder(make);testCanvasPrivateSerialization(make);postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/private-canvas-state.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
