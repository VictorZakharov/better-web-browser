use super::*;

#[test]
fn offscreen_canvas_web_idl_dimensions_contexts_and_transfer_ownership() {
    let source = format!(
        "{}\ntestOffscreenBindings();",
        include_str!("../../../../tests/canvas/offscreen-bindings.js")
    );
    let (_, outcome) = execute_html(&format!("<script>{source}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn workers_share_the_offscreen_canvas_binding_and_transfer_contract() {
    let source = format!(
        "{}\ntestOffscreenBindings();postMessage('passed');",
        include_str!("../../../../tests/canvas/offscreen-bindings.js")
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/offscreen-bindings.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
