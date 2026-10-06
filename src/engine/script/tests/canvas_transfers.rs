use super::*;

const TRANSFER_CONTRACT: &str = include_str!("../../../../tests/canvas/transfer-bindings.js");

#[test]
fn canvas_transfer_steps_run_after_graph_getters_in_list_order() {
    let (_, outcome) = execute_html(&format!(
        "<script>{TRANSFER_CONTRACT}\ntestCanvasTransfers();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_clone_wire_data_and_private_bindings_are_isolated_from_author_hooks() {
    let (_, outcome) = execute_html(&format!(
        "<script>{TRANSFER_CONTRACT}\ntestCanvasCloneIsolation();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn workers_share_the_canvas_transfer_and_private_clone_contract() {
    let source = format!(
        "{TRANSFER_CONTRACT}\ntestCanvasTransfers();testCanvasCloneIsolation();postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/transfer-bindings.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
