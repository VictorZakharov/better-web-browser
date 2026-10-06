use super::*;

#[test]
fn offscreen_native_images_match_scalar_sampling_and_snapshot_ownership() {
    let source = format!(
        "{}\ntestCanvasImagePaint((w,h)=>new OffscreenCanvas(w,h));postMessage('passed');",
        include_str!("../../../../tests/canvas/image-paint.js")
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/image-paint.js",
        &source,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
