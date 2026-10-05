use super::*;

#[test]
fn offscreen_indexed_blending_uses_real_mrt_pixels_and_shared_validation() {
    let source = format!(
        "{}\ntestIndexedBlend(()=>new OffscreenCanvas(4,4));postMessage('passed');",
        include_str!("../../../../tests/webgl/indexed-blend.js")
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/indexed-blend.js",
        &source,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(runtime.is_some());
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"passed\""]);
    assert!(initial.fetch_actions.is_empty());
}
