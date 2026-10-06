use super::*;

const USABILITY: &str = include_str!("../../../../tests/canvas/image-usability.js");

#[test]
fn canvas_image_usability_is_private_and_precedes_pattern_validation() {
    let (_, outcome) = execute_html(&format!(
        "<script>{USABILITY}\ntestCanvasImageUsability((w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}});testHtmlImageUsability();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_canvas_image_sources_use_the_same_private_brand_and_snapshot_rules() {
    let source = format!(
        "{USABILITY}\ntestCanvasImageUsability((w,h)=>new OffscreenCanvas(w,h));postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/image-usability.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
