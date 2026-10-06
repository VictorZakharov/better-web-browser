use super::*;

const METRICS: &str = include_str!("../../../../tests/canvas/text-metrics-bindings.js");

#[test]
fn canvas_text_metrics_are_private_readonly_baseline_relative_snapshots() {
    let (_, outcome) = execute_html(&format!(
        "<script>{METRICS}\nconst make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};testCanvasTextMetrics(make);</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_canvas_text_metrics_keep_private_brands_and_baseline_distances() {
    let source = format!(
        "{METRICS}\nconst make=(w,h)=>new OffscreenCanvas(w,h);testCanvasTextMetrics(make);postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-text-metrics.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
