use super::*;

#[test]
fn canvas_native_images_match_scalar_sampling_across_blends_transforms_and_clips() {
    let script = format!(
        "{}\ntestCanvasImagePaint((w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}});",
        include_str!("../../../../tests/canvas/image-paint.js")
    );
    let (_, outcome) = execute_html(&format!("<script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_exact_center_blits_match_small_scalar_draws_in_window_and_worker() {
    const SOURCE: &str = include_str!("../../../../tests/canvas/image-blit.js");
    let (_, outcome) = execute_html(&format!(
        "<script>{SOURCE};if(testCanvasImageBlits((w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}})!==73)throw Error('case count');</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/image-blits.js",
        &format!(
            "{SOURCE};if(testCanvasImageBlits((w,h)=>new OffscreenCanvas(w,h))!==73)throw Error('worker case count');postMessage('passed');"
        ),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"passed\""]);
}
