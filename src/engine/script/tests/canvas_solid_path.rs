use super::*;

const SOLID_PATH: &str = include_str!("../../../../tests/canvas/solid-path.js");

#[test]
fn fused_solid_path_paint_equals_clipped_fallback_across_geometry_pen_and_alpha() {
    let (_, outcome) = execute_html(&format!(
        "<script>{SOLID_PATH}\ntestSolidPathPainting();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn workers_share_native_fused_fill_and_stroke_ownership_and_pixels() {
    let source = format!("{SOLID_PATH}\npostMessage(testSolidPathPainting().cases);");
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/solid-path.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["120"]);
}
