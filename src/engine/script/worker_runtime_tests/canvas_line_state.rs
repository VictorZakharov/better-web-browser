use super::*;

#[test]
fn offscreen_line_styles_convert_and_iterate_before_updating_state() {
    let source = format!(
        "{}\ntestCanvasLineState(()=>new OffscreenCanvas(1,1));postMessage('passed');",
        include_str!("../../../../tests/canvas/line-state.js")
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-line-state.js",
        &source,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
