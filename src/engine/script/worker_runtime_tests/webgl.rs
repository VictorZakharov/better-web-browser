use super::*;

#[test]
fn worker_offscreen_webgl_executes_on_native_owner_and_exports_real_pixels() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"
            const canvas = new OffscreenCanvas(2,2);
            const gl = canvas.getContext('webgl', {preserveDrawingBuffer:true});
            if (!(gl instanceof WebGLRenderingContext)) throw new Error('WebGL worker context unavailable');
            gl.clearColor(0,0,1,1); gl.clear(gl.COLOR_BUFFER_BIT);
            const bitmap = canvas.transferToImageBitmap();
            const copy = new OffscreenCanvas(2,2).getContext('2d'); copy.drawImage(bitmap,0,0);
            postMessage([...copy.getImageData(0,0,1,1).data].join());
            onmessage = () => {
                const pixels = new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
                postMessage([...pixels].join());
                close();
            };
        "#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages, ["\"0,0,255,255\""]);
    let mut runtime = runtime.unwrap();
    let completion = runtime.dispatch_message("null");
    assert!(completion.errors.is_empty(), "{:?}", completion.errors);
    assert_eq!(completion.messages, ["\"0,0,0,0\""]);
    assert!(completion.closed);
}
