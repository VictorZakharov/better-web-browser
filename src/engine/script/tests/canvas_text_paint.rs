use super::*;
const PAINT: &str = include_str!("../../../../tests/canvas/text-paint.js");

#[test]
fn native_glyph_paint_matches_scalar_masks_with_composites_clips_shadows_and_opaque_surfaces() {
    let (_, outcome) = execute_html(&format!(
        "<script>{PAINT}\nconst make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};if(testCanvasTextPaint(make)!==208)throw Error('missing cases');</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_native_glyph_paint_matches_the_same_scalar_contract() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/text-paint.js",
        &format!(
            "{PAINT}\nif(testCanvasTextPaint((w,h)=>new OffscreenCanvas(w,h))!==208)throw Error('missing cases');postMessage('passed');"
        ),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
