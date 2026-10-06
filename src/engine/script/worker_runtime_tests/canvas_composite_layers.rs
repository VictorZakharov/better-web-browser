use super::*;

#[test]
fn offscreen_native_layers_preserve_porter_duff_blends_clips_and_shadow_order() {
    let source = format!(
        "{}\ntestCanvasCompositeLayers((width,height)=>new OffscreenCanvas(width,height));postMessage('passed');",
        include_str!("../../../../tests/canvas/composite-layers.js")
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/composite-layers.js",
        &source,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
