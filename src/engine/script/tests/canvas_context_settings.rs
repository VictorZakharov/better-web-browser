use super::*;

const SETTINGS: &str = include_str!("../../../../tests/canvas/context-settings.js");

#[test]
fn canvas_settings_own_real_opaque_pixels_and_dictionary_conversions() {
    let (_, outcome) = execute_html(&format!(
        "<script>{SETTINGS}\nconst make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};testCanvasContextSettings(make);testOpaqueCanvasPixels(make);testOpaqueCanvasNativeFallbackParity(make);</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_canvas_settings_own_opaque_pixels_and_transfer_snapshots() {
    let source = format!(
        "{SETTINGS}\nconst make=(w,h)=>new OffscreenCanvas(w,h);testCanvasContextSettings(make);testOpaqueCanvasPixels(make);testOpaqueCanvasNativeFallbackParity(make);postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-context-settings.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
