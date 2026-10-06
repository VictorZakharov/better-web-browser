use super::*;

#[test]
fn canvas_image_draw_applies_web_idl_before_snapshot_and_layer_creation() {
    let script = format!(
        "{}\ntestCanvasImageDrawBindings((w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}});",
        include_str!("../../../../tests/canvas/image-draw-bindings.js")
    );
    let (_, outcome) = execute_html(&format!("<script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn workers_apply_image_draw_web_idl_before_snapshot_and_layer_creation() {
    let source = format!(
        "{}\ntestCanvasImageDrawBindings((w,h)=>new OffscreenCanvas(w,h));postMessage('passed');",
        include_str!("../../../../tests/canvas/image-draw-bindings.js")
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/image-draw-bindings.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
