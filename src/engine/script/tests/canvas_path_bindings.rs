use super::*;

const PATH_BINDINGS: &str = include_str!("../../../../tests/canvas/path-bindings.js");

#[test]
fn canvas_path_web_idl_converts_once_before_geometry_and_ctm() {
    let (_, outcome) = execute_html(&format!(
        "<script>{PATH_BINDINGS}\ntestCanvasPathBindings();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_round_rect_converts_radii_union_dictionaries_and_iterables() {
    let (_, outcome) = execute_html(&format!(
        "<script>{PATH_BINDINGS}\ntestCanvasRadiusBindings();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_path2d_retains_completed_svg_commands_before_parse_error() {
    let (_, outcome) = execute_html(&format!(
        "<script>{PATH_BINDINGS}\ntestCanvasSvgPrefix();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn rounded_paths_preserve_reflected_winding_and_fresh_subpath_semantics() {
    let (_, outcome) = execute_html(&format!(
        "<script>{PATH_BINDINGS}\ntestCanvasPathGeometry();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn workers_share_canvas_path_argument_and_svg_prefix_contracts() {
    let source = format!(
        "{PATH_BINDINGS}\ntestCanvasPathBindings();testCanvasRadiusBindings();testCanvasSvgPrefix();testCanvasPathGeometry();postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/path-bindings.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
