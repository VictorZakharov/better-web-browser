use super::*;

#[test]
fn offscreen_canvas_and_path_transforms_share_dictionary_and_numeric_conversion() {
    let source = format!(
        "{}\ntestMatrixContracts(()=>new OffscreenCanvas(1,1));postMessage('passed');",
        include_str!("../../../../tests/canvas/matrix-contracts.js")
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-matrices.js",
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

#[test]
fn offscreen_patterns_share_opaque_snapshots_and_matrix_dictionary_conversion() {
    let source = format!(
        "{}\ntestPatternContracts(()=>new OffscreenCanvas(1,1));postMessage('passed');",
        include_str!("../../../../tests/canvas/pattern-contracts.js")
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-patterns.js",
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
