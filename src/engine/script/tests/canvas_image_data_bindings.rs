use super::*;

const BINDINGS: &str = include_str!("../../../../tests/canvas/image-data-bindings.js");

#[test]
fn legacy_persistent_image_data_records_still_decode_as_rgba8() {
    let (_, outcome) = execute_html(
        r#"<script>
        const image=__deserializeClone('{"t":"image-data","id":1,"w":1,"h":1,"p":"DCI4/w=="}');
        if(!(image instanceof ImageData)||image.width!==1||image.height!==1||
            image.pixelFormat!=='rgba-unorm8'||image.data.join(',')!=='12,34,56,255')
            throw Error('legacy ImageData storage');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_image_data_has_private_idl_storage_and_clone_buffer_identity() {
    let (_, outcome) = execute_html(&format!(
        "<script>{BINDINGS}\nconst make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};testImageDataConstruction();testCanvasImageDataBindings(make);testImageDataStructuredClone();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_image_data_has_private_idl_storage_and_clone_buffer_identity() {
    let source = format!(
        "{BINDINGS}\nconst make=(w,h)=>new OffscreenCanvas(w,h);testImageDataConstruction();testCanvasImageDataBindings(make);testImageDataStructuredClone();postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-image-data-bindings.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}
